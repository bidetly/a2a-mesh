//! A2A client: send and task tracking (DESIGN.md §8.1).
//!
//! `mesh_send` calls A2A `SendMessage` and waits up to `timeout_ms`. A
//! `Message` response, or a `Task` reaching terminal state inside the window,
//! returns `outcome: "message"` or `outcome: "completed"`. Otherwise the node
//! spawns a background task via `TaskManager::spawn` and returns
//! `outcome: "pending"` carrying both the A2A `task_id` and MCP `mcp_task_id`.
//!
//! The spawned future consumes A2A updates and republishes them as MCP task
//! state. Transport preference: SSE via `SendStreamingMessage` when the peer's
//! card advertises `streaming`, otherwise polling with backoff.
