//! `a2a-mesh` — a Rust MCP server for agent-to-agent messaging over A2A.
//!
//! A single binary run as an MCP server by a coding agent. Each instance is
//! three things in one process (DESIGN.md §1):
//!
//! - an MCP server for the one agent that spawned it,
//! - an A2A server accepting tasks from other agents,
//! - an A2A client sending tasks to other agents.
//!
//! Discovery runs through etcd: each instance writes its agent card to a
//! lease-held key and reads the key prefix to see peers.
//!
//! This file handles config, wiring, and tokio setup.

mod bridge;
mod discovery;
mod error;
mod identity;
mod inbound;
mod outbound;
mod tools;

fn main() {
    // Logs go to stderr — stdout carries MCP stdio traffic and writing to it
    // corrupts the protocol stream (DESIGN.md §10).
    let config = a2a_mesh::config::Config::load().unwrap_or_else(|error| {
        eprintln!("configuration error: {error}");
        std::process::exit(2);
    });
    let advertised_name =
        a2a_mesh::security::advertised_name(&config).expect("bind IP supplies an advertised name");
    let _identity = a2a_mesh::security::SecurityIdentity::generate(
        &advertised_name,
        config.security.certificate_lifetime,
    )
    .unwrap_or_else(|error| {
        eprintln!("security initialization error: {error}");
        std::process::exit(2);
    });
    if config.has_insecure_remote_etcd_endpoint() {
        eprintln!("warning: insecure plaintext etcd was explicitly enabled; use HTTPS for non-loopback etcd");
    }
    todo!("wiring and tokio setup")
}
