//! Utilities for keeping track of and validating authentication attempts.

use hyper::{Response, Body};

use serde::Deserialize;
use tokio::time::{Instant, Duration};

use super::{Result, utils};

impl super::AppState {
    /// Return a `Duration` representing the period of time after which a token
    /// is no longer considered valid.
    fn get_token_age(&self) -> Duration {
        Duration::from_secs(self.ctx.cfg.security.auth_timeout as u64)
    }

    /// Generate a unique token with which to challenge the client for the password.
    pub(super) fn gen_login_token(&self) -> u64 {
        let token = rand::random();
        self.login_tokens.write().insert(token, Instant::now());
        token
    }

    /// Remove any login tokens that are older than the specified maximum.
    pub(super) fn clear_login_tokens(&self) {
        let mut logins = self.login_tokens.write();
        let num_logins = logins.len();
        logins.retain(|_, creation_time| creation_time.elapsed() < self.get_token_age());
        let num_cleared = num_logins - logins.len();
        if num_cleared > 0 {
            self.ctx.log.status(format!(
                "cleared {} login token{}",
                num_cleared,
                if num_cleared == 1 { "" } else { "s" }
            ))
        }
    }

    /// Generate a response to a login attempt.
    pub(super) fn login(&self, body: Vec<u8>) -> Result<Response<Body>> {
        let password = match self.ctx.cfg.security.login_password.clone() {
            Some(pw) => pw,
            None => {
                self.ctx.log.info("cannot authenticate - no admin password provided");
                return self.error_401();
            }
        };

        /// Describes the format of authentication requests.
        #[derive(Deserialize)]
        struct LoginData {
            /// The token provided by `/login`.
            token: String,
            /// A value that must be equal to `hash(token + ":" + password)`
            /// in order to correctly authenticate.
            hash: String,
        }

        let LoginData { token, hash } = serde_json::from_slice(&body)
            .or_else(|_| self.error_400())?;
        let token: u64 = token.parse()
            .or_else(|_| self.error_400())?;
        let logins = self.login_tokens.read();
        let creation_time = logins.get(&token).ok_or(())
            .or_else(|_| self.error_401())?;
        
        if creation_time.elapsed() > self.get_token_age() {
            self.error_401()?;
        }

        let msg = format!("{}:{}", token, password);
        if utils::sha256(&msg) != hash {
            self.ctx.log.info("authentication attempt was rejected");
            self.error_401()?
        } else {
            self.ctx.log.info("user was authenticated");
            Ok(Self::redirect("/admin"))
        }
    }
}
