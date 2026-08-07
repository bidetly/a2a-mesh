//! MCP tool definitions (DESIGN.md §3).
//!
//! Every tool returns a JSON object as `structuredContent`, with a short text
//! rendering alongside it. Announcement is required: every peer-facing tool
//! returns `NotAnnounced` before the first successful `mesh_announce`.
//!
//! Identity:   `mesh_announce`, `mesh_whoami`
//! Discovery:  `mesh_list_peers`, `mesh_describe_peer`
//! Sending:    `mesh_send`, `mesh_get_task`, `mesh_cancel_task`,
//!             `mesh_list_tasks`, `mesh_respond`
//! Receiving:  `mesh_inbox`, `mesh_reply`
//!
//! Resources (DESIGN.md §3.5):
//! - `mesh://peers`      — the peer list
//! - `mesh://inbox`      — pending inbound messages (resource-updated fires here)
//! - `mesh://task/{id}`  — a task's full transcript
//! - `mesh://blob/{id}`  — a large artifact payload
