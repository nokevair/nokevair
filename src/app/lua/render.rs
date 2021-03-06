//! The renderer refers to the set of dynamic scripts used to describe
//! information about the worldstate by rendering it to HTML.
//! 
//! For example, when the user accesses a URL like `/10/people?i=foobar`,
//! the server would do the following:
//! 
//! - Load a Lua function (call it `f`) from the file `render/people/focus.lua`.
//! - Load version 10 of the state.
//! - Call `f(state, "foobar")` and convert the result into JSON to get a
//!   Tera template context.
//! - Invoke the template `render/people/format.html.tera` with that context.
//! 
//! Lua and Tera are both fast, and template contexts will generally be
//! fairly small, so this whole process should be doable for every request.
//! It might be good to set up some kind of caching system for commonly
//! accessed pages.

use hyper::{Response, Body};
use rlua::Value as LV;

use std::fs;
use std::path::PathBuf;

use crate::conv;
use crate::utils::SourceChain;
use super::{Ctx, Version, Result, AppState};

/// Apply a function to certain paths in the `render` directory
/// which correspond to renderer entries.
/// 
/// When `f` is invoked, the first argument is the name of the
/// entry, and the second is the path to its directory.
pub fn with_entries<F: FnMut(String, PathBuf)>(app_ctx: &Ctx, mut f: F) {
    // Get an iterator over the contents in the `render` directory.
    let dir = match fs::read_dir(&app_ctx.cfg.paths.render) {
        Ok(dir) => dir,
        Err(e) => {
            app_ctx.log.err(format_args!("failed to read render dir: {}", e));
            return
        }
    };
    
    for entry in dir {
        // Attempt to get information about this directory entry.
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                app_ctx.log.err(format_args!("failed while reading render dir: {}", e));
                continue
            }
        };

        // If this entry isn't a directory, ignore it.
        let path = entry.path();
        if !path.is_dir() {
            continue
        }

        // Get the name of that directory.
        let name = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => {
                app_ctx.log.err(format_args!(
                    "failed to load focus at '{}': invalid UTF-8", path.display()));
                continue
            }
        };

        f(name, path);
    }
}

impl super::Backend {
    /// Remove all focus functions from the Lua registry and from `self.focuses`
    /// and attempt to ensure they are garbage collected.
    pub(super) fn unload_focuses(&mut self) {
        // This invokes the Drop implementation, allowing Lua to know
        // that the keys are no longer in use.
        self.focuses.clear();
        // Garbage collect all the old functions.
        self.lua.context(|ctx| ctx.expire_registry_values());
    }

    /// Add functions from the Lua registry and to `self.focuses` corresponding
    /// to the return values of executing `/render/*/focus.lua`.
    pub(super) fn load_focuses(&mut self, app_ctx: &Ctx) {
        with_entries(app_ctx, |name, mut path| {
            // Read the file `focus.lua` inside that directory.
            path.push("focus.lua");
            let code = match fs::read_to_string(&path) {
                Ok(code) => code,
                Err(e) => {
                    app_ctx.log.err(format_args!(
                        "failed to read file '{}': {}",
                        path.display(),
                        e
                    ));
                    return
                }
            };

            // Evaluate the contents of that file as Lua code
            // and store the returned function in the registry.
            let focuses = &mut self.focuses;
            let res = self.lua.context(|ctx| {
                let focus_fn = ctx.load(&code)
                    .eval::<rlua::Function>()?;
                let key = ctx.create_registry_value(focus_fn)?;
                focuses.insert(name, key);
                Ok::<(), rlua::Error>(())
            });

            if let Err(e) = res {
                app_ctx.log.err(format_args!(
                    "lua ('{}' -> focus):\n{}",
                    path.display(),
                    SourceChain(e),
                ));
            }
        });

        let len = self.focuses.len();
        app_ctx.log.info(format_args!(
            "loaded {} focus function{}",
            len,
            if len == 1 { "" } else { "s" }
        ));
    }

    /// Invoke the renderer to generate a response for a specified path,
    /// specified version of the state, and specified query parameter.
    pub(super) fn render(
        &mut self,
        ver: Version,
        name: &str,
        query_param: Option<String>,
        app_state: &AppState,
    ) -> Result<Response<Body>> {
        /// Helper macro to generate a description of the render request
        macro_rules! render_call {
            () => {
                match &query_param {
                    Some(param) => format!("'{}' with arg '{}'", name, param),
                    None => format!("'{}'", name),
                }
            }
        }

        self.ensure_loaded(ver, &app_state.ctx);

        self.lua.context(|ctx| {
            // Look up the focus function
            let focus_fn_key = self.focuses.get(name)
                .ok_or(())
                .or_else(|_| app_state.error_404())?;
            let focus_fn: rlua::Function = ctx.registry_value(focus_fn_key)
                .or_else(|_| app_state.error_500("invalid focus fn key"))?;
            
            // Look up the state
            let state_key = self.state_versions.get(ver.as_usize())
                .ok_or(())
                .or_else(|_| app_state.error_404_no_state(ver))?;
            let state: LV = ctx.registry_value(state_key)
                .or_else(|_| app_state.error_500("invalid state key"))?;
            
            // Apply the function to the state and query param
            let ctx: Option<rlua::Table> = focus_fn.call((state, query_param.clone()))
                .or_else(|e| app_state.error_500(format_args!(
                    "lua (focus {}):\n{}",
                    render_call!(),
                    SourceChain(e),
                )))?;
            
            // As a special case, return 404 if the focus returns nil
            let ctx = ctx.ok_or(()).or_else(|_| app_state.error_404())?;

            // Convert the context from a Lua value to JSON
            let ctx = conv::lua_to_json(LV::Table(ctx))
                .or_else(|e| app_state.error_500(format_args!(
                    "lua (focus {} -> JSON):\n{}",
                    render_call!(),
                    SourceChain(e)
                )))?;
            
            // Convert the JSON to a Tera context.
            let ctx = tera::Context::from_serialize(ctx)
                .or_else(|e| app_state.error_500(format_args!(
                    "tera (focus {} -> Tera ctx):\n{}",
                    render_call!(),
                    SourceChain(e),
                )))?;
                
            // TODO: add additional variables to the context, such as what version
            // of the state we're using
            
            let template = format!("render/{}.html", name);
            app_state.render(&template, &ctx)
        })
    }
}
