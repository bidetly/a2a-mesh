//! Canonical agent handles and best-effort local process metadata.
//!
//! Handles are deliberately ASCII-only.  This avoids Unicode normalization and
//! URL/path ambiguities at the discovery claim boundary.

use std::{
    fmt,
    io::Read,
    process::{Command, Stdio},
    str::FromStr,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use uuid::Uuid;

use crate::dto::ProcessMetadata;

const MAX_LOCAL_LEN: usize = 63;
const MAX_HOST_LEN: usize = 253;
const MAX_LABEL_LEN: usize = 63;
const MAX_HANDLE_LEN: usize = MAX_LOCAL_LEN + 1 + MAX_HOST_LEN;
const MAX_CLAIM_SEGMENT_LEN: usize = (MAX_HANDLE_LEN * 4).div_ceil(3);
const MAX_PROBE_BYTES: usize = 4096;

/// A validated canonical mesh handle, `local@host`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Handle {
    local: String,
    host: String,
}

/// Why a handle cannot be used as a mesh identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HandleError {
    InvalidSeparator,
    InvalidLocal,
    InvalidHost,
    TooLong,
    InvalidClaimEncoding,
}

impl fmt::Display for HandleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSeparator => "handle must contain exactly one @ separator",
            Self::InvalidLocal => "handle local component is not canonical lowercase ASCII labels",
            Self::InvalidHost => {
                "handle host component is not canonical lowercase ASCII dot labels"
            }
            Self::TooLong => "handle exceeds the canonical length limit",
            Self::InvalidClaimEncoding => "handle claim segment is not a canonical encoded handle",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for HandleError {}

impl Handle {
    /// Parses an already-canonical `local@host` handle. Input is never trimmed,
    /// case-folded, percent-decoded, or Unicode-normalized.
    pub fn parse(input: &str) -> Result<Self, HandleError> {
        if input.len() > MAX_HANDLE_LEN {
            return Err(HandleError::TooLong);
        }
        let Some((local, host)) = input.split_once('@') else {
            return Err(HandleError::InvalidSeparator);
        };
        if host.contains('@') {
            return Err(HandleError::InvalidSeparator);
        }
        if !valid_local(local) {
            return Err(HandleError::InvalidLocal);
        }
        if !valid_host(host) {
            return Err(HandleError::InvalidHost);
        }
        Ok(Self {
            local: local.into(),
            host: host.into(),
        })
    }

    pub fn local(&self) -> &str {
        &self.local
    }
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Makes the deterministic collision form (`local-N@host`) through the
    /// same validator used for externally supplied handles.
    pub fn with_discriminator(&self, discriminator: u32) -> Result<Self, HandleError> {
        Self::parse(&format!("{}-{discriminator}@{}", self.local, self.host))
    }

    /// A reversible base64url-without-padding segment safe to append to a key
    /// prefix. The decoded value is reparsed before it is returned.
    pub fn claim_key_segment(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.to_string().as_bytes())
    }

    pub fn from_claim_key_segment(segment: &str) -> Result<Self, HandleError> {
        if segment.is_empty()
            || segment.len() > MAX_CLAIM_SEGMENT_LEN
            || !segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(HandleError::InvalidClaimEncoding);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(segment)
            .map_err(|_| HandleError::InvalidClaimEncoding)?;
        let decoded = std::str::from_utf8(&bytes).map_err(|_| HandleError::InvalidClaimEncoding)?;
        let handle = Self::parse(decoded).map_err(|_| HandleError::InvalidClaimEncoding)?;
        (handle.claim_key_segment() == segment)
            .then_some(handle)
            .ok_or(HandleError::InvalidClaimEncoding)
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.local, self.host)
    }
}

impl FromStr for Handle {
    type Err = HandleError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

fn valid_local(value: &str) -> bool {
    valid_labels(value, b'-', MAX_LOCAL_LEN)
}
fn valid_host(value: &str) -> bool {
    valid_labels(value, b'.', MAX_HOST_LEN)
}
fn valid_labels(value: &str, separator: u8, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.split(char::from(separator)).all(|label| {
            !label.is_empty()
                && label.len() <= MAX_LABEL_LEN
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Stable local process identity and independently optional observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessIdentity {
    pub instance_id: Uuid,
    pub metadata: ProcessMetadata,
}

impl ProcessIdentity {
    pub fn collect() -> Self {
        Self {
            instance_id: Uuid::new_v4(),
            metadata: collect_process_metadata(),
        }
    }
}

/// Collects safe local facts. Individual probe failures deliberately remain
/// absent rather than preventing announcement or registration.
pub fn collect_process_metadata() -> ProcessMetadata {
    ProcessMetadata {
        cwd: std::env::current_dir()
            .ok()
            .and_then(|path| safe_text(path.to_string_lossy().as_ref())),
        repository: git_output(&["remote", "get-url", "origin"])
            .and_then(|remote| safe_repository_identity(&remote)),
        branch: git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            .and_then(|value| safe_text(&value)),
        pid: Some(std::process::id()),
        hostname: command_output("hostname", &[]).and_then(|value| safe_text(&value)),
    }
}

fn git_output(arguments: &[&str]) -> Option<String> {
    command_output("git", arguments)
}
fn command_output(program: &str, arguments: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let mut bytes = Vec::with_capacity(MAX_PROBE_BYTES + 1);
    stdout
        .take((MAX_PROBE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_PROBE_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    if !child.wait().ok()?.success() {
        return None;
    }
    let value = std::str::from_utf8(&bytes)
        .ok()?
        .trim_end_matches(['\r', '\n']);
    safe_text(value)
}

fn safe_text(value: &str) -> Option<String> {
    (!value.is_empty() && value.len() <= MAX_PROBE_BYTES && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

/// Converts only recognized GitHub origin forms to a credential-free identity.
fn safe_repository_identity(remote: &str) -> Option<String> {
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("https://github.com/"))?;
    if remote.contains('?')
        || remote.contains('#')
        || remote.contains('@')
            && !remote.starts_with("git@github.com:")
            && !remote.starts_with("ssh://git@github.com/")
    {
        return None;
    }
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    };
    (valid(owner) && valid(repo)).then(|| format!("github.com/{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_canonical_ascii_handles() {
        for value in ["planner@dev-laptop", "a@a", "build-2@ci.example.test"] {
            assert_eq!(Handle::parse(value).unwrap().to_string(), value);
        }
        for value in [
            "",
            "local",
            "@host",
            "local@",
            "a@b@c",
            "UPPER@host",
            "a_b@host",
            "a/@host",
            "a%2f@host",
            "a@host..test",
            "a@höst",
            " a@host",
        ] {
            assert!(Handle::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn enforces_component_and_total_limits() {
        let local63 = "a".repeat(63);
        assert!(Handle::parse(&format!("{local63}@host")).is_ok());
        assert!(Handle::parse(&format!("{}@host", "a".repeat(64))).is_err());
        let host253 = format!(
            "{}.{}",
            "a".repeat(63),
            ["b".repeat(63), "c".repeat(63), "d".repeat(61)].join(".")
        );
        assert_eq!(host253.len(), 253);
        assert!(Handle::parse(&format!("a@{host253}")).is_ok());
        let maximal = format!("{local63}@{host253}");
        assert_eq!(maximal.len(), MAX_HANDLE_LEN);
        assert!(Handle::parse(&maximal).is_ok());
        let host254 = format!(
            "{}.{}",
            "a".repeat(63),
            ["b".repeat(63), "c".repeat(63), "d".repeat(62)].join(".")
        );
        assert_eq!(host254.len(), 254);
        assert!(Handle::parse(&format!("a@{host254}")).is_err());
    }

    #[test]
    fn claim_segment_is_safe_and_reversible() {
        let handle = Handle::parse("planner@dev-laptop").unwrap();
        let segment = handle.claim_key_segment();
        assert!(!segment.contains(['/', '%', '=']));
        assert_eq!(Handle::from_claim_key_segment(&segment).unwrap(), handle);
        assert!(Handle::from_claim_key_segment("not/a/segment").is_err());
        assert!(Handle::from_claim_key_segment("_").is_err());
        assert!(Handle::from_claim_key_segment(&"a".repeat(MAX_CLAIM_SEGMENT_LEN + 1)).is_err());
    }

    #[test]
    fn discriminator_revalidates_the_local_component() {
        let handle = Handle::parse("planner@host").unwrap();
        assert_eq!(
            handle.with_discriminator(2).unwrap().to_string(),
            "planner-2@host"
        );
        let long = Handle::parse(&format!("{}@host", "a".repeat(62))).unwrap();
        assert!(long.with_discriminator(2).is_err());
    }

    #[test]
    fn repository_identity_removes_remote_syntax_and_rejects_secrets() {
        assert_eq!(
            safe_repository_identity("git@github.com:owner/repo.git").as_deref(),
            Some("github.com/owner/repo")
        );
        assert_eq!(
            safe_repository_identity("https://github.com/owner/repo").as_deref(),
            Some("github.com/owner/repo")
        );
        for unsafe_remote in [
            "https://token@github.com/owner/repo",
            "https://github.com/owner/repo?token=x",
            "ssh://evil@github.com/owner/repo",
            "file:///private/repo",
        ] {
            assert!(safe_repository_identity(unsafe_remote).is_none());
        }
    }

    #[test]
    fn metadata_probes_are_best_effort_and_process_identity_is_stable() {
        assert!(command_output("a2a-mesh-command-that-does-not-exist", &[]).is_none());
        let identity = ProcessIdentity::collect();
        assert_eq!(identity.instance_id, identity.instance_id);
        assert_eq!(identity.instance_id.get_version_num(), 4);
        assert_eq!(identity.metadata.pid, Some(std::process::id()));
        // Collection has no fallible return: absent optional probe values do not
        // prevent process identity allocation.
        let _ = collect_process_metadata();
    }
}
