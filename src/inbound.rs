//! A2A server: `AgentExecutor` impl and the inbound message queue (DESIGN.md §6).
//!
//! A peer's A2A message is routed by `a2a-server-lf` to our `AgentExecutor`,
//! which puts the message on a bounded in-memory queue (default capacity 64)
//! and returns a task in `submitted` state. The node then emits an MCP
//! resource-updated notification for `mesh://inbox` (best-effort).
//!
//! When the queue is full, further inbound messages are rejected with an A2A
//! error saying the agent is saturated. A message unread past
//! `inbound.deadline` (default 15 minutes) is failed with a reason, so a peer
//! is never blocked indefinitely (DESIGN.md §6.2–§6.3).
