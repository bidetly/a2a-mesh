//! Handle, card construction, and announce state (DESIGN.md §4).
//!
//! A handle is `local-name@host`: the local name comes from `mesh_announce`,
//! the host part is the machine's hostname. Multiple instances per machine is
//! normal, so collisions are expected; a live collision appends a discriminator
//! (`payments-refactor-2@dev-laptop`) and reports the actual handle with
//! `renamed: true`.
//!
//! The etcd key is a process-unique UUID rather than the handle, so a rename
//! through re-announcing does not orphan a key (DESIGN.md §4.1).

/// A mesh handle, `local@host`.
#[derive(Debug, Clone)]
pub struct Handle {
    pub local: String,
    pub host: String,
}
