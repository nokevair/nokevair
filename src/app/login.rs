//! Utilities for keeping track of and validating authentication attempts.

use hyper::{Response, Body};

use serde::Deserialize;
use tokio::time::{Instant, Duration};

use std::sync::PoisonError;

use super::{Result, utils};

/// How long do tokens remain valid?
pub const TOKEN_AGE: Duration = Duration::from_secs(60);

impl super::AppState {
    /// Generate a unique token with which to challenge the client for the password.
    pub(super) fn gen_login_token(&self) -> u64 {
        let token = rand::random();
        self.log.info(format!("generated token ({})", token));
        self.login_tokens.write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(token, Instant::now());
        token
    }

    /// Remove any login tokens that are older than the specified maximum.
    pub(super) fn clear_login_tokens(&self) {
        let mut logins = self.login_tokens.write()
            .unwrap_or_else(PoisonError::into_inner);
        let num_logins = logins.len();
        logins.retain(|_, creation_time| creation_time.elapsed() < TOKEN_AGE);
        let num_cleared = num_logins - logins.len();
        if num_cleared > 0 {
            self.log.info(format!(
                "cleared {} login token{}.",
                num_cleared,
                if num_cleared > 1 { "s" } else { "" }
            ))
        }
    }

    /// Generate a response to a login attempt.
    pub(super) fn login(&self, body: Vec<u8>) -> Result<Response<Body>> {
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
        let logins = self.login_tokens.read()
            .unwrap_or_else(PoisonError::into_inner);
        let creation_time = logins.get(&token).ok_or(())
            .or_else(|_| self.error_401())?;
        
        if creation_time.elapsed() > TOKEN_AGE {
            self.error_401()?;
        }

        // TODO: use a better password, and read it from a file or something
        let msg = format!("{}:foobar", token);
        if utils::sha256(&msg) != hash {
            self.log.info("authentication attempt was rejected");
            self.error_401()?
        } else {
            self.log.info("user was authenticated");
            Ok(Self::redirect("/admin"))
        }
    }
}