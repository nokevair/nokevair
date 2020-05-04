//! Utilities for error/info logging.

use std::fmt::Display;
use std::sync::{RwLock, PoisonError};

/// Represents the type of a log message.
#[derive(Clone, Copy)]
enum MessageKind {
    /// An error message.
    Error,
    /// An informational message.
    Info,
}

/// Represents a message in the log.
struct Message {
    kind: MessageKind,
    is_deleted: bool,
    body: String,
}

/// Keeps track of all messages written to the log.
pub struct Log {
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
}