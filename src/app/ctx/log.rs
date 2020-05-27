//! Utilities for error/info logging.

use serde::Serialize;

use std::fmt::Display;
use std::sync::{RwLock, PoisonError};

/// Represents the type of a log message.
#[derive(Clone, Copy, Serialize)]
pub enum MessageKind {
    /// An error message.
    #[serde(rename="error")]
    Error,
    /// An informational message.
    #[serde(rename="info")]
    Info,
    /// A confirmation of a frequent/routine action.
    #[serde(rename="status")]
    Status,
}

/// Represents a message in the log.
#[derive(Clone, Serialize)]
pub struct Message {
    /// What kind of message is this?
    pub kind: MessageKind,
    /// Has the admin already marked this message as deleted?
    pub is_deleted: bool,
    /// The content of the message
    pub body: String,
}

/// Keeps track of all messages written to the log.
pub struct Log {
    /// The list of messages, stored in the order they were created.
    messages: RwLock<Vec<Message>>,
}

impl Log {
    /// Initialize the log.
    pub fn new() -> Self {
        Self {
            messages: RwLock::default(),
        }
    }

    /// Push a message to the log.
    fn add_message(&self, msg: Message) {
        self.messages.write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(msg);
    }

    /// Add an error message to the log.
    pub fn err<M: Display>(&self, msg: M) {
        let body = format!("{}", msg);
        eprintln!("\x1b[1;31merror: \x1b[39;49m{}", body);
        self.add_message(Message {
            kind: MessageKind::Error,
            is_deleted: false,
            body,
        });
    }

    /// Add an info message to the log.
    pub fn info<M: Display>(&self, msg: M) {
        let body = format!("{}", msg);
        eprintln!("\x1b[1;33minfo: \x1b[39;49m{}", body);
        self.add_message(Message {
            kind: MessageKind::Info,
            is_deleted: false,
            body,
        });
    }

    /// Add a status message to the log.
    pub fn status<M: Display>(&self, msg: M) {
        let body = format!("{}", msg);
        eprintln!("\x1b[1;32mstatus: \x1b[39;49m{}", body);
        self.add_message(Message {
            kind: MessageKind::Status,
            is_deleted: false,
            body,
        });
    }
    
    /// Call a function on each message in order opposite to when they were created.
    pub fn for_each<F: FnMut(&Message)>(&self, mut f: F) {
        let messages = self.messages.read()
            .unwrap_or_else(PoisonError::into_inner);
        for msg in messages.iter().rev() {
            f(msg);
        }
    }
}

/// Parameters used to filter the log for certain messages.
#[derive(Clone, Copy)]
pub struct Filter {
    /// Whether to keep error messages
    error: bool,
    /// Whether to keep info messages
    info: bool,
    /// Whether to keep status messages
    status: bool,
    /// Whether to keep deleted messages
    deleted: bool,
}

impl Filter {
    /// Parse this from a byte slice like b"yyyn".
    pub fn from_body(body: &[u8]) -> Self {
        Self {
            error:   body.get(0) == Some(&b'y'),
            info:    body.get(1) == Some(&b'y'),
            status:  body.get(2) == Some(&b'y'),
            deleted: body.get(3) == Some(&b'y'),
        }
    }

    /// Check whether the message is permitted by the filter.
    pub fn permits(self, msg: &Message) -> bool {
        (self.deleted || !msg.is_deleted) && match msg.kind {
            MessageKind::Error => self.error,
            MessageKind::Info => self.info,
            MessageKind::Status => self.status,
        }
    }
}
