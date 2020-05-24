//! Handles the process of simulating changes to the world state.
//! Like the focus, the simulation is a dynamically loaded Lua program.
//! However, there are a number of differences:
//! 
//! - Rather than keeping the simulation function in the Lua registry,
//!   we just re-read the program every time we perform the simulation.
//! - Rather than using the renderer's copies of the world state, we
//!   re-read it from a file every time we perform the simulation.
//!   This way, any accidental changes to the state caused by the focuses
//!   will not affect it.
//! - Unlike the renderer, simulation code is never released to the public.
//!   State files are released to the public, but only in bulk and at
//!   infrequent intervals.
//! 
//! The simulation is executed in a separate thread and uses a new Lua
//! instance every time.

use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, PoisonError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::conv;
use super::{Ctx, Version};

/// Stores config info for the simulation.
pub struct Sim {
    /// The path to the file containing the Lua simulation code.
    file: RwLock<Option<PathBuf>>,
    /// Used to let the previously-created simulation thread know that it should exit.
    cancel_previous: Mutex<Arc<AtomicBool>>, // TODO: arc_swap?
}

impl Sim {
    /// Create the initial version of the simulation state.
    pub fn new(ctx: &Ctx) -> Self {
        Self {
            // TODO: change this to RwLock::default and make it configurable
            // in the admin panel
            file: RwLock::new(Some(ctx.cfg.paths.sim.join("0.lua"))),
            // This is a dummy value and will be discarded after the
            // first simulation starts.
            cancel_previous: Mutex::default(),
        }
    }

    /// Execute one iteration of the simulation in a new thread. If the former
    /// simulation thread is not done executing, let it know that it should stop.
    pub fn run(&self, app_ctx: Ctx) {
        // Create a new cancellation flag for use in the new thread
        let is_cancelled = {
            let mut cancel_previous = self.cancel_previous.lock()
                .unwrap_or_else(PoisonError::into_inner);
            // Make sure the previous thread knows it should be cancelled
            cancel_previous.store(true, Ordering::Relaxed);
            *cancel_previous = Arc::new(AtomicBool::new(false));
            Arc::clone(&*cancel_previous)
        };

        // Get the path of the simulation file
        let lua_file = self.file.read()
            .unwrap_or_else(PoisonError::into_inner);
        let lua_file = match &*lua_file {
            Some(f) => f.to_string_lossy().into_owned(),
            None => {
                app_ctx.log.status("no simulation file specified");
                return
            }
        };

        let time_limit = Duration::from_secs(
            app_ctx.cfg.runtime.sim_rate.load(Ordering::Relaxed) as u64);
        
        thread::Builder::new()
            .name("simulation".into())
            .spawn(move || {
                let lua = super::create_lua_state(&app_ctx);

                let start_time = Instant::now();
                
                // Every 1000 lua instructions, check that this thread hasn't been cancelled
                // or run out of time
                let mut triggers = rlua::HookTriggers::default();
                triggers.every_nth_instruction = Some(1000);
                lua.set_hook(triggers, move |_, _| {
                    if is_cancelled.load(Ordering::Relaxed) {
                        Err(rlua::Error::RuntimeError(String::from("cancelled")))
                    } else if start_time.elapsed() > time_limit {
                        Err(rlua::Error::RuntimeError(String::from("out of time")))
                    } else {
                        Ok(())
                    }
                });
                
                let res = lua.context::<_, rlua::Result<()>>(|ctx| {
                    use rlua::Value as LV;

                    // Read the MessagePack file containing the latest version of the state.
                    let next_ver = Version::next_available(&app_ctx);
                    let current_state = match next_ver.previous() {
                        None => {
                            app_ctx.log.status("no state files found; using fresh state");
                            LV::Nil
                        }
                        Some(ver) => {
                            let state_path = ver.path(&app_ctx);
                            app_ctx.log.status(format_args!(
                                "using '{}' for simulation",
                                state_path.display(),
                            ));

                            let mut state_file = match File::open(&state_path) {
                                Ok(file) => file,
                                Err(e) => {
                                    app_ctx.log.err(format_args!(
                                        "file could not be opened: {}",
                                        e
                                    ));
                                    return Ok(())
                                }
                            };

                            let mpv = match conv::bytes_to_msgpack(&mut state_file) {
                                Ok(mpv) => mpv,
                                Err(e) => {
                                    app_ctx.log.err(format_args!(
                                        "file could not be read as msgpack: {}",
                                        e
                                    ));
                                    return Ok(())
                                }
                            };

                            match conv::msgpack_to_lua(mpv, ctx) {
                                Ok(lv) => lv,
                                Err(e) => {
                                    app_ctx.log.err(format_args!(
                                        "file could not be converted to lua object: {:?}",
                                        e
                                    ));
                                    return Ok(())
                                }
                            }
                        }
                    };

                    // Read the Lua file that defines the simulation.
                    let sim_code = match fs::read_to_string(&lua_file) {
                        Ok(code) => code,
                        Err(e) => {
                            app_ctx.log.err(format_args!(
                                "could not read simulation code in '{}': {}",
                                lua_file,
                                e
                            ));
                            return Ok(())
                        }
                    };

                    // Evaluate the Lua code to get a function.
                    let sim_fn = ctx.load(&sim_code)
                        .set_name(&lua_file)?
                        .eval::<rlua::Function>()?;
                    
                    // Apply this function to the state to get the new state.
                    let new_state = sim_fn.call::<_, LV>((current_state, lua_file))?;

                    // Convert this state back into a MessagePack object.
                    let mpv = conv::lua_to_msgpack(new_state)?;

                    let real_next_ver = Version::next_available(&app_ctx);

                    if next_ver != real_next_ver {
                        app_ctx.log.info(format_args!(
                            "writing to '{}' instead of '{}' as was originally intended",
                            real_next_ver.path(&app_ctx).display(),
                            next_ver.path(&app_ctx).display(),
                        ))
                    }

                    let path = real_next_ver.path(&app_ctx);
                    let mut new_state_file = match File::create(&path) {
                        Ok(file) => file,
                        Err(e) => {
                            app_ctx.log.err(format_args!(
                                "could not create file '{}': {}",
                                path.display(),
                                e
                            ));
                            return Ok(());
                        }
                    };

                    if let Err(e) = conv::msgpack_to_bytes(&mut new_state_file, &mpv) {
                        app_ctx.log.err(format_args!(
                            "could not write state to file '{}': {}",
                            path.display(),
                            e
                        ));
                    } else {
                        app_ctx.log.status(format_args!(
                            "wrote new state file '{}'",
                            path.display()
                        ));
                    }

                    Ok(())
                });

                if let Err(e) = res {
                    app_ctx.log.err(format!("lua error during simulation: {:?}", e));
                }
            }).expect("failed to start simulation thread");
    }
}
