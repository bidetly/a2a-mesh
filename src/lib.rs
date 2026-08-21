//! Public types and contracts for a2a-mesh.

pub mod dto;

pub use dto::{
    ArtifactDescriptor, ContentKind, ContentPart, DiscoveryHealth, DiscoverySnapshot, HandleClaim,
    IdentityProjection, JsonRpcInterface, PeerProjection, ProcessMetadata, Registration,
    ResourceDescriptor, TaskDescriptor, TaskState, ToolDescriptor, TransportSecurity,
    TransportSecurityMode,
};
