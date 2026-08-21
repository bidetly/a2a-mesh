//! Stable, serialization-safe public data-transfer objects for a2a-mesh.
//!
//! These types intentionally do not depend on routing, storage, or transport
//! implementations.  They define the JSON contracts consumed by the MCP,
//! resource, discovery, and A2A bridge layers.

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::DateTime;

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

pub type Metadata = BTreeMap<String, serde_json::Value>;

/// A discovery registration schema version supported by this crate.
pub const REGISTRATION_SCHEMA_VERSION: u32 = 1;
/// A handle-claim schema version supported by this crate.
pub const HANDLE_CLAIM_SCHEMA_VERSION: u32 = 1;

/// TLS policy advertised for a mesh endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportSecurityMode {
    Plaintext,
    PinnedTls,
}

/// Certificate pinning material advertised with a registration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TransportSecurity {
    pub mode: TransportSecurityMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spki_sha256: Option<String>,
    /// RFC 3339 certificate expiration timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_expires_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportSecurityWire {
    mode: TransportSecurityMode,
    spki_sha256: Option<String>,
    certificate_expires_at: Option<String>,
}

impl TryFrom<TransportSecurityWire> for TransportSecurity {
    type Error = String;

    fn try_from(value: TransportSecurityWire) -> Result<Self, Self::Error> {
        match (value.mode, value.spki_sha256, value.certificate_expires_at) {
            (TransportSecurityMode::Plaintext, None, None) => Ok(Self {
                mode: TransportSecurityMode::Plaintext,
                spki_sha256: None,
                certificate_expires_at: None,
            }),
            (TransportSecurityMode::Plaintext, _, _) => {
                Err("plaintext security must not contain certificate pinning material".into())
            }
            (TransportSecurityMode::PinnedTls, Some(pin), Some(expiration)) => {
                validate_lowercase_spki_sha256(&pin)?;
                DateTime::parse_from_rfc3339(&expiration)
                    .map_err(|_| "certificate_expires_at must be RFC 3339".to_owned())?;
                Ok(Self {
                    mode: TransportSecurityMode::PinnedTls,
                    spki_sha256: Some(pin),
                    certificate_expires_at: Some(expiration),
                })
            }
            (TransportSecurityMode::PinnedTls, _, _) => {
                Err("pinned-tls security requires spki_sha256 and certificate_expires_at".into())
            }
        }
    }
}

impl<'de> Deserialize<'de> for TransportSecurity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TransportSecurityWire::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

/// Observed process properties; unavailable observations remain absent.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessMetadata {
    pub cwd: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub pid: Option<u32>,
}

/// Lease-bound, versioned discovery registration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    #[serde(deserialize_with = "deserialize_registration_version")]
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub handle: String,
    pub endpoint: String,
    /// The only A2A binding advertised by the MVP.
    pub interface: JsonRpcInterface,
    /// Monotonically increasing version of this instance's registration/card.
    pub registration_version: u64,
    pub process: ProcessMetadata,
    pub security: TransportSecurity,
}

/// A versioned ownership record for the separate handle key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HandleClaim {
    #[serde(deserialize_with = "deserialize_handle_claim_version")]
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub handle: String,
}

/// JSON-RPC details exposed by an A2A registration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcInterface {
    #[serde(rename = "protocol", deserialize_with = "deserialize_jsonrpc_protocol")]
    pub protocol: String,
    #[serde(deserialize_with = "deserialize_a2a_jsonrpc_version")]
    pub version: String,
}

/// Health of the discovery snapshot from which a peer projection was made.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryHealth {
    Fresh,
    Stale,
    Unavailable,
}

/// Explicit discovery snapshot status. Stale peers are retained, not hidden.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverySnapshot {
    pub health: DiscoveryHealth,
    pub revision: u64,
    /// RFC 3339 time at which a discovery refresh last succeeded.
    pub last_successful_refresh: Option<String>,
    /// Age of the last usable snapshot in milliseconds.
    pub stale_age_ms: Option<u64>,
}

/// Mesh identity projection for a peer or inbound sender.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityProjection {
    pub instance_id: Option<Uuid>,
    pub handle: Option<String>,
    pub identity_verified: bool,
}

/// A discovered peer with local reachability and snapshot status visible.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeerProjection {
    pub identity: IdentityProjection,
    pub endpoint: String,
    pub reachable: bool,
    pub snapshot: DiscoverySnapshot,
    pub registration: Registration,
}

/// A mesh tool advertised to callers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

/// A mesh resource exposed to callers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDescriptor {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub media_type: Option<String>,
}

/// Each A2A task state is preserved as its distinct wire value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    AuthRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
}

/// A task projection. `task_id` is an opaque process-generated mesh ID; the
/// separately named peer ID is never substituted for it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDescriptor {
    pub task_id: String,
    pub a2a_task_id: Option<String>,
    pub context_id: Option<String>,
    pub state: TaskState,
    pub parts: Vec<ContentPart>,
    pub artifacts: Vec<ArtifactDescriptor>,
}

/// A content part used in messages and artifacts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum ContentPart {
    /// UTF-8 text content.
    Text {
        text: String,
        media_type: Option<String>,
        filename: Option<String>,
        #[serde(default)]
        metadata: Metadata,
    },
    /// Base64-encoded binary content.
    Raw {
        #[serde(deserialize_with = "deserialize_base64")]
        raw: String,
        media_type: Option<String>,
        filename: Option<String>,
        #[serde(default)]
        metadata: Metadata,
    },
    /// A referenced resource URI.
    Url {
        url: String,
        media_type: Option<String>,
        filename: Option<String>,
        #[serde(default)]
        metadata: Metadata,
    },
    /// Structured JSON content.
    Data {
        data: serde_json::Value,
        media_type: Option<String>,
        filename: Option<String>,
        #[serde(default)]
        metadata: Metadata,
    },
}

/// The four A2A content forms represented by [`ContentPart`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Text,
    Raw,
    Url,
    Data,
}

/// Artifact metadata and its content chunks.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDescriptor {
    pub artifact_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub parts: Vec<ContentPart>,
    pub append: bool,
    #[serde(rename = "lastChunk")]
    pub final_chunk: bool,
    #[serde(default)]
    pub metadata: Metadata,
}

fn deserialize_registration_version<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let version = u32::deserialize(deserializer)?;
    (version == REGISTRATION_SCHEMA_VERSION)
        .then_some(version)
        .ok_or_else(|| {
            serde::de::Error::custom(format!("unsupported registration schema_version {version}"))
        })
}

fn deserialize_handle_claim_version<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let version = u32::deserialize(deserializer)?;
    (version == HANDLE_CLAIM_SCHEMA_VERSION)
        .then_some(version)
        .ok_or_else(|| {
            serde::de::Error::custom(format!("unsupported handle claim schema_version {version}"))
        })
}

fn deserialize_jsonrpc_protocol<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    (value == "jsonrpc")
        .then_some(value)
        .ok_or_else(|| serde::de::Error::custom("interface protocol must be jsonrpc"))
}

fn deserialize_a2a_jsonrpc_version<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    (value == "1.0")
        .then_some(value)
        .ok_or_else(|| serde::de::Error::custom("interface version must be 1.0"))
}

fn validate_lowercase_spki_sha256(fingerprint: &str) -> Result<(), String> {
    let valid = fingerprint.len() == 64
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    valid.then_some(()).ok_or_else(|| {
        "spki_sha256 must be a 64-character lowercase hexadecimal SHA-256 fingerprint".to_owned()
    })
}

fn deserialize_base64<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    BASE64
        .decode(&raw)
        .map_err(|_| serde::de::Error::custom("raw must be standard base64"))?;
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration_json() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "instance_id": "6f0d9b1b-f4c3-4e5e-9a8d-7f9f559898a4",
            "handle": "planner@laptop",
            "endpoint": "https://127.0.0.1:4567",
            "interface": { "protocol": "jsonrpc", "version": "1.0" },
            "registration_version": 4,
            "process": { "cwd": "/work/project", "repository": null, "branch": "trunk", "pid": 42 },
            "security": { "mode": "pinned-tls", "spki_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "certificate_expires_at": "2027-01-01T00:00:00Z" }
        })
    }

    #[test]
    fn registration_round_trips_with_stable_field_names() {
        let parsed: Registration = serde_json::from_value(registration_json()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), registration_json());
    }

    #[test]
    fn rejects_unknown_registration_version_and_invalid_pin() {
        let mut unknown_version = registration_json();
        unknown_version["schema_version"] = serde_json::json!(2);
        assert!(serde_json::from_value::<Registration>(unknown_version).is_err());
        let mut uppercase_pin = registration_json();
        uppercase_pin["security"]["spki_sha256"] =
            serde_json::json!("A123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
        assert!(serde_json::from_value::<Registration>(uppercase_pin).is_err());
    }

    #[test]
    fn task_states_are_distinct_lowercase_values() {
        let states = [
            TaskState::Submitted,
            TaskState::Working,
            TaskState::InputRequired,
            TaskState::AuthRequired,
            TaskState::Completed,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::Rejected,
        ];
        let encoded: Vec<_> = states
            .into_iter()
            .map(|state| serde_json::to_string(&state).unwrap())
            .collect();
        assert_eq!(
            encoded,
            [
                "\"submitted\"",
                "\"working\"",
                "\"input-required\"",
                "\"auth-required\"",
                "\"completed\"",
                "\"failed\"",
                "\"canceled\"",
                "\"rejected\""
            ]
        );
    }

    #[test]
    fn content_parts_are_a_tagged_one_of() {
        let valid = serde_json::json!({"kind": "text", "text": "hello", "media_type": null, "filename": null, "metadata": {}});
        assert!(serde_json::from_value::<ContentPart>(valid).is_ok());
        let contradictory = serde_json::json!({"kind": "text", "text": "hello", "url": "https://example.test", "media_type": null, "filename": null, "metadata": {}});
        assert!(serde_json::from_value::<ContentPart>(contradictory).is_err());
    }

    #[test]
    fn artifact_uses_a2a_last_chunk_wire_name() {
        let artifact = ArtifactDescriptor {
            artifact_id: "artifact-1".into(),
            name: None,
            description: None,
            parts: vec![],
            append: false,
            final_chunk: true,
            metadata: Metadata::new(),
        };
        let value = serde_json::to_value(artifact).unwrap();
        assert_eq!(value["lastChunk"], true);
        assert!(value.get("final_chunk").is_none());
    }

    #[test]
    fn rejects_ambiguous_or_unknown_fields() {
        let mut value = registration_json();
        value["certificateFingerprint"] = serde_json::json!("alias");
        assert!(serde_json::from_value::<Registration>(value).is_err());
    }
}
