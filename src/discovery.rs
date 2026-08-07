//! Discovery via etcd: lease, put, and cached prefix reads (DESIGN.md §5).
//!
//! One key per instance under `/a2a-mesh/agents/{uuid}`, holding the serialized
//! agent card plus mesh metadata. The key is written by `mesh_announce` and by
//! nothing else.
//!
//! `LeasePool` owns lease renewal in a background task; there is no
//! deregistration path — the key exists exactly as long as the lease is
//! renewed. `CacheClient` serves prefix reads from a watch-maintained local
//! store, since reads (list_peers, handle resolution) dominate this workload.
//!
//! If etcd is unreachable at startup the node still serves MCP and reports
//! peers as unavailable (DESIGN.md §5.5).
