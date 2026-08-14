//! Error types (DESIGN.md §9).
//!
//! Errors are read by a model that will otherwise retry the same call. Each
//! variant should state what happened and what to do next.

use std::time::Duration;

use crate::identity::Handle;

/// The error type for mesh operations.
///
/// This planned public error surface is defined before its command wiring;
/// retain it while the scaffold is completed rather than adding artificial
/// call sites solely to satisfy a lint.
#[expect(dead_code, reason = "command wiring has not been implemented yet")]
#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    /// This instance has not announced itself, so it has no identity and no
    /// peers can see it.
    #[error(
        "This instance has not announced itself, so it has no identity and no \
         peers can see it. Call mesh_announce with a name, description, and the \
         skills this agent can offer."
    )]
    NotAnnounced,

    /// No peer with the given handle is known.
    #[error("Unknown peer {handle:?}.")]
    UnknownPeer { handle: Handle },

    /// A peer that is listed but could not be reached (DESIGN.md §5.6).
    #[error("Peer {handle:?} is listed but could not be reached.")]
    PeerUnreachable {
        handle: Handle,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// No task with the given id is tracked.
    #[error("Task {id} not found.")]
    TaskNotFound { id: String },

    /// The task is terminal and cannot be modified; follow up in the same
    /// context with a new task.
    #[error(
        "Task {id} is {state}. A2A tasks cannot be modified once terminal. To \
         follow up, call mesh_send with context_id {context_id:?}."
    )]
    TaskTerminal {
        id: String,
        state: String,
        context_id: String,
    },

    /// The inbound queue is full (DESIGN.md §6.2).
    #[error("Inbox is full (capacity {capacity}); the agent is saturated.")]
    InboxFull { capacity: usize },

    /// etcd could not be reached (DESIGN.md §5.5).
    #[error("etcd is unavailable.")]
    EtcdUnavailable {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// An operation exceeded its wait window.
    #[error("Timed out after {waited:?}.")]
    Timeout { waited: Duration },

    /// An unexpected internal error.
    #[error("Internal error: {0}")]
    Internal(Box<dyn std::error::Error + Send + Sync>),
}
