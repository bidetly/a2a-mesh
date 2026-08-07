//! The task bridge: A2A task <-> MCP task (DESIGN.md §8).
//!
//! MCP's Tasks extension (SEP-2663), implemented in `rmcp` 3.x as
//! `rmcp::task_manager`, has a state model that corresponds closely to A2A's:
//!
//! | A2A `TaskState`                    | MCP task state                     |
//! |------------------------------------|------------------------------------|
//! | `submitted`, `working`             | `working`                          |
//! | `input-required`, `auth-required`  | `input_required`                   |
//! | `completed`                        | terminal, `result`                 |
//! | `failed`, `canceled`, `rejected`   | terminal, `error` (distinct codes) |
//!
//! This module also maps content: A2A `Part` (oneof text/raw/url/data, with
//! mediaType/filename/metadata) to MCP content, preserving bytes, `mediaType`,
//! and `filename` in both directions. `raw` payloads above 256 KiB are held in
//! memory and exposed as a `mesh://blob/{id}` resource (DESIGN.md §8.2).
