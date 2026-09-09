//! Public types and contracts for a2a-mesh.

pub mod config;
pub mod dto;
pub mod identity;
pub mod security;

pub use dto::{
    ArtifactDescriptor, ContentKind, ContentPart, DiscoveryHealth, DiscoverySnapshot, HandleClaim,
    IdentityProjection, JsonRpcInterface, PeerProjection, ProcessMetadata, Registration,
    ResourceDescriptor, TaskDescriptor, TaskState, ToolDescriptor, TransportSecurity,
    TransportSecurityMode,
};
