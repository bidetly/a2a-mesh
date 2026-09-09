//! Canonical agent handles and best-effort local process metadata.
//!
//! Handles are deliberately ASCII-only.  This avoids Unicode normalization and
//! URL/path ambiguities at the discovery claim boundary.

use std::{fmt, path::Path, str::FromStr, sync::OnceLock, time::Duration};
#[cfg(unix)]
use std::{
    io::Read,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::Instant,
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
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

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
    /// same validator used for externally supplied handles. Always succeeds
    /// for any validly-parsed handle and any `discriminator`: the suffix is
    /// at most 11 bytes (`-` plus up to 10 digits, for `u32::MAX`), well
    /// under `MAX_LOCAL_LEN`, so the local component is shortened as needed
    /// to make room rather than ever overflowing the limit.
    pub fn with_discriminator(&self, discriminator: u32) -> Result<Self, HandleError> {
        let suffix = format!("-{discriminator}");
        // `local` is pure ASCII (enforced by `valid_local` at construction), so
        // every byte index is a valid `str` slice boundary. Its first byte is
        // alnum, so trimming hyphens exposed by the cut can never empty it.
        let budget = (MAX_LOCAL_LEN - suffix.len()).min(self.local.len());
        let base = self.local[..budget].trim_end_matches('-');
        Self::parse(&format!("{base}{suffix}@{}", self.host))
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

static INSTANCE_ID: OnceLock<Uuid> = OnceLock::new();

/// Stable local process identity and independently optional observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessIdentity {
    pub instance_id: Uuid,
    pub metadata: ProcessMetadata,
}

impl ProcessIdentity {
    /// Metadata is re-collected on each call; `instance_id` is allocated once
    /// per process and reused thereafter, so repeated announces keep a
    /// stable identity while still refreshing observable facts.
    pub fn collect() -> Self {
        Self {
            instance_id: *INSTANCE_ID.get_or_init(Uuid::new_v4),
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
            .and_then(|path| safe_cwd_component(&path)),
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
    command_output_with_timeout(program, arguments, PROBE_TIMEOUT)
}
/// Without a bounded way to contain and terminate a probe's entire process
/// tree on this platform (the Unix implementation below uses its own
/// process group; there is no equivalent here, e.g. a Windows kill-on-close
/// Job Object), a descendant that inherits the stdout pipe could block the
/// reader past any deadline indefinitely. Skip the probe rather than risk
/// that -- metadata collection is best-effort throughout, so an
/// always-absent probe here is within contract.
#[cfg(not(unix))]
fn command_output_with_timeout(
    _program: &str,
    _arguments: &[&str],
    _timeout: Duration,
) -> Option<String> {
    None
}

/// Runs `program`, capturing up to `MAX_PROBE_BYTES` of stdout within one
/// overall wall-clock deadline that bounds both the read and the child's
/// exit. The child is spawned as its own process group so that, once the
/// deadline is reached, the *entire* process tree -- including a descendant
/// that inherited or duplicated the stdout pipe -- can be killed together;
/// killing only the direct child would leave such a descendant (and the
/// reader thread still blocked on its open pipe) running indefinitely.
/// Closing stdout does not imply the process has exited, so a successful
/// read still waits (within the same deadline) for the child's actual exit
/// before trusting its status.
#[cfg(unix)]
fn command_output_with_timeout(
    program: &str,
    arguments: &[&str],
    timeout: Duration,
) -> Option<String> {
    use std::os::unix::process::CommandExt;

    let deadline = Instant::now() + timeout;
    let mut child = Command::new(program)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;

    let (result_tx, result_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::with_capacity(MAX_PROBE_BYTES + 1);
        let outcome = stdout
            .take((MAX_PROBE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = result_tx.send(outcome);
    });

    let read = result_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let Ok(Ok(bytes)) = read else {
        kill_process_tree(&mut child);
        let _ = child.wait();
        let _ = reader.join();
        return None;
    };
    // EOF was reached, so the reader thread has already finished; joining it
    // now cannot block further.
    let _ = reader.join();
    if bytes.len() > MAX_PROBE_BYTES {
        kill_process_tree(&mut child);
        let _ = child.wait();
        return None;
    }

    // The process may keep running after closing stdout, so give it the
    // remainder of the same deadline to actually exit before forcing it.
    let remaining = deadline.saturating_duration_since(Instant::now());
    let status = match wait_within(&mut child, remaining) {
        Some(status) => status,
        None => {
            kill_process_tree(&mut child);
            let _ = child.wait();
            return None;
        }
    };
    if !status.success() {
        return None;
    }
    let value = std::str::from_utf8(&bytes)
        .ok()?
        .trim_end_matches(['\r', '\n']);
    safe_text(value)
}

/// Polls for the child's exit, returning its status if it exits within
/// `budget`, or `None` if the budget elapses first.
#[cfg(unix)]
fn wait_within(child: &mut Child, budget: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + budget;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Kills the child's entire process group, so a descendant that inherited or
/// duplicated its stdout pipe cannot outlive it.
#[cfg(unix)]
fn kill_process_tree(child: &mut Child) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGKILL: i32 = 9;
    // SAFETY: `kill` is the standard libc syscall wrapper, which this target
    // already links against. Signaling a process group that has already
    // exited returns ESRCH; the return value is ignored either way since
    // there is nothing further to do on failure.
    let _ = unsafe { kill(-(child.id() as i32), SIGKILL) };
}

fn safe_text(value: &str) -> Option<String> {
    (!value.is_empty() && value.len() <= MAX_PROBE_BYTES && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

/// Extracts only the final path component (never a full path) as a safe
/// published value. `None` for root-like paths with no final component and
/// for non-UTF-8 names — omitted rather than fabricated with replacement
/// characters.
fn safe_cwd_component(path: &Path) -> Option<String> {
    path.file_name()?.to_str().and_then(safe_text)
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

        // A 63-byte local is shortened, not rejected.
        let maximal = Handle::parse(&format!("{}@host", "a".repeat(63))).unwrap();
        assert_eq!(
            maximal.with_discriminator(2).unwrap().to_string(),
            format!("{}-2@host", "a".repeat(61))
        );

        // The largest possible u32 discriminator still fits.
        assert_eq!(
            maximal.with_discriminator(u32::MAX).unwrap().to_string(),
            format!("{}-4294967295@host", "a".repeat(52))
        );

        // A cut landing exactly on a hyphen must not leave it dangling.
        let hyphen_at_boundary = Handle::parse(&format!("{}-bb@host", "a".repeat(60))).unwrap();
        assert_eq!(
            hyphen_at_boundary
                .with_discriminator(2)
                .unwrap()
                .to_string(),
            format!("{}-2@host", "a".repeat(60))
        );
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
        let first = ProcessIdentity::collect();
        let second = ProcessIdentity::collect();
        assert_eq!(first.instance_id, second.instance_id);
        assert_eq!(first.instance_id.get_version_num(), 4);
        assert_eq!(first.metadata.pid, Some(std::process::id()));
        if let Some(cwd) = &collect_process_metadata().cwd {
            assert!(
                !cwd.contains('/'),
                "cwd must be a bare segment, got {cwd:?}"
            );
        }
    }

    #[test]
    fn safe_cwd_component_extracts_the_final_path_segment() {
        assert_eq!(
            safe_cwd_component(Path::new("/home/alice/project")).as_deref(),
            Some("project")
        );
        assert_eq!(safe_cwd_component(Path::new("/")), None);
        assert_eq!(safe_cwd_component(Path::new("/home/alice/..")), None);
    }

    #[cfg(unix)]
    #[test]
    fn safe_cwd_component_omits_non_utf8_final_segments() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

        let invalid_utf8_name =
            OsStr::from_bytes(&[b'p', b'r', b'o', b'j', 0xFF, b'e', b'c', b't']);
        assert_eq!(safe_cwd_component(Path::new(invalid_utf8_name)), None);
    }

    #[cfg(unix)]
    #[test]
    fn command_output_returns_captured_stdout_within_timeout() {
        let result =
            command_output_with_timeout("sh", &["-c", "printf hi"], Duration::from_secs(1));
        assert_eq!(result.as_deref(), Some("hi"));
    }

    // Also proves the descendant `sleep` (not just the direct `sh`) is
    // actually killed, not merely orphaned: `sleep` inherits the same
    // stdout pipe as `sh`, so if it survived, the reader thread would still
    // be blocked in `read_to_end` on that pipe for the remainder of its 5s
    // sleep, and joining it (inside `command_output_with_timeout`) would
    // make this test take close to 5s instead of returning promptly.
    #[cfg(unix)]
    #[test]
    fn command_output_abandons_a_stuck_child_after_its_deadline() {
        let start = Instant::now();
        let result = command_output_with_timeout(
            "sh",
            &["-c", "printf hi; sleep 5"],
            Duration::from_millis(50),
        );
        assert_eq!(result, None);
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(30),
            "elapsed: {elapsed:?} — returned suspiciously fast; the helper \
             process may never have started, so the timeout path was not \
             actually exercised"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "elapsed: {elapsed:?} — appears to have waited for the child's sleep"
        );
    }

    // The child closes stdout almost immediately (so the read succeeds well
    // within the deadline) but keeps running afterward. The overall
    // deadline must still bound the wait for its actual exit.
    #[cfg(unix)]
    #[test]
    fn command_output_bounds_wait_even_after_stdout_closes_early() {
        let start = Instant::now();
        let result = command_output_with_timeout(
            "sh",
            &["-c", "printf hi; exec 1>&-; sleep 5"],
            Duration::from_millis(50),
        );
        assert_eq!(result, None);
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "elapsed: {elapsed:?} — appears to have waited for the child's exit \
             instead of bounding it to the deadline"
        );
    }

    // On platforms without a bounded way to contain and kill a probe's
    // whole process tree, the probe must never run at all -- a descendant
    // that inherits stdout could otherwise block past any deadline. This
    // only compiles and runs on such targets (e.g. Windows); it cannot be
    // exercised from this development environment.
    #[cfg(not(unix))]
    #[test]
    fn command_output_is_disabled_without_bounded_process_tree_cleanup() {
        assert_eq!(
            command_output_with_timeout("echo", &["hi"], Duration::from_secs(1)),
            None
        );
    }
}
