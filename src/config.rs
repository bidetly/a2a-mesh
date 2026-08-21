//! Typed, validated runtime configuration.
//!
//! Configuration is merged in increasing precedence: a TOML file, environment,
//! then command-line options. This module deliberately has no credential fields.

use std::{env, fs, net::IpAddr, path::PathBuf, time::Duration};

use serde::Deserialize;
use thiserror::Error;

const MIB: u64 = 1024 * 1024;
const KIB: u64 = 1024;
const MAX_DURATION_SECS: u64 = 365 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub etcd_endpoints: Vec<String>,
    pub listen_address: IpAddr,
    pub advertised_host: Option<String>,
    pub limits: Limits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub inbound_capacity: usize,
    pub inbound_deadline: Duration,
    pub max_outbound_tracking: Duration,
    pub terminal_retention: Duration,
    pub tasks_per_peer: usize,
    pub tasks_per_context: usize,
    pub total_blob_bytes: u64,
    pub inline_content_threshold: u64,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration file {path} could not be read: {source}")]
    ReadFile {
        path: String,
        source: std::io::Error,
    },
    #[error("configuration file {path} is invalid: {source}")]
    ParseFile {
        path: String,
        source: toml::de::Error,
    },
    #[error("invalid value for {field}: {message}")]
    Invalid {
        field: &'static str,
        message: String,
    },
    #[error("unknown command-line option: {0}")]
    UnknownOption(String),
    #[error("missing value for command-line option: {0}")]
    MissingOptionValue(String),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            etcd_endpoints: vec!["http://127.0.0.1:2379".into()],
            listen_address: "127.0.0.1".parse().expect("valid default address"),
            advertised_host: None,
            limits: Limits {
                inbound_capacity: 64,
                inbound_deadline: Duration::from_secs(15 * 60),
                max_outbound_tracking: Duration::from_secs(24 * 60 * 60),
                terminal_retention: Duration::from_secs(24 * 60 * 60),
                tasks_per_peer: 256,
                tasks_per_context: 256,
                total_blob_bytes: 512 * MIB,
                inline_content_threshold: 256 * KIB,
            },
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    etcd: Option<FileEtcd>,
    listen: Option<FileListen>,
    limits: Option<FileLimits>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEtcd {
    endpoints: Option<Vec<String>>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileListen {
    address: Option<String>,
    advertised_host: Option<String>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLimits {
    inbound_capacity: Option<usize>,
    inbound_deadline_secs: Option<u64>,
    max_outbound_tracking_secs: Option<u64>,
    terminal_retention_secs: Option<u64>,
    tasks_per_peer: Option<usize>,
    tasks_per_context: Option<usize>,
    total_blob_bytes: Option<u64>,
    inline_content_threshold: Option<u64>,
}

impl Config {
    /// Load and validate config from `args` (including the program name) and `environment`.
    pub fn load_from<I, S>(args: I, environment: &[(String, String)]) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let config_path = option_value(&args, "--config")?
            .map(PathBuf::from)
            .or_else(|| lookup(environment, "A2A_MESH_CONFIG").map(PathBuf::from));
        let mut config = Self::default();
        if let Some(path) = config_path {
            config.apply_file(&path)?;
        }
        config.apply_env(environment)?;
        config.apply_cli(&args)?;
        config.validate()?;
        Ok(config)
    }

    /// Load from the process command line and environment before startup wiring.
    pub fn load() -> Result<Self, ConfigError> {
        let environment = env::vars().collect::<Vec<_>>();
        Self::load_from(env::args(), &environment)
    }

    fn apply_file(&mut self, path: &PathBuf) -> Result<(), ConfigError> {
        let text = fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        let file: FileConfig = toml::from_str(&text).map_err(|source| ConfigError::ParseFile {
            path: path.display().to_string(),
            source,
        })?;
        if let Some(etcd) = file.etcd {
            if let Some(v) = etcd.endpoints {
                self.etcd_endpoints = v;
            }
        }
        if let Some(listen) = file.listen {
            if let Some(v) = listen.address {
                self.listen_address = parse_ip("listen.address", &v)?;
            }
            if let Some(v) = listen.advertised_host {
                self.advertised_host = Some(v);
            }
        }
        if let Some(v) = file.limits {
            self.apply_limits(v);
        }
        Ok(())
    }
    fn apply_limits(&mut self, v: FileLimits) {
        if let Some(x) = v.inbound_capacity {
            self.limits.inbound_capacity = x;
        }
        if let Some(x) = v.inbound_deadline_secs {
            self.limits.inbound_deadline = Duration::from_secs(x);
        }
        if let Some(x) = v.max_outbound_tracking_secs {
            self.limits.max_outbound_tracking = Duration::from_secs(x);
        }
        if let Some(x) = v.terminal_retention_secs {
            self.limits.terminal_retention = Duration::from_secs(x);
        }
        if let Some(x) = v.tasks_per_peer {
            self.limits.tasks_per_peer = x;
        }
        if let Some(x) = v.tasks_per_context {
            self.limits.tasks_per_context = x;
        }
        if let Some(x) = v.total_blob_bytes {
            self.limits.total_blob_bytes = x;
        }
        if let Some(x) = v.inline_content_threshold {
            self.limits.inline_content_threshold = x;
        }
    }
    fn apply_env(&mut self, env: &[(String, String)]) -> Result<(), ConfigError> {
        if let Some(v) = lookup(env, "A2A_MESH_ETCD_ENDPOINTS") {
            self.etcd_endpoints = csv("etcd.endpoints", v)?;
        }
        if let Some(v) = lookup(env, "A2A_MESH_LISTEN_ADDRESS") {
            self.listen_address = parse_ip("listen.address", v)?;
        }
        if let Some(v) = lookup(env, "A2A_MESH_ADVERTISED_HOST") {
            self.advertised_host = Some(v.to_owned());
        }
        macro_rules! limit {
            ($env:literal, $field:literal, $target:ident) => {
                if let Some(v) = lookup(env, $env) {
                    self.limits.$target = parse(v, $field)?;
                }
            };
        }
        limit!(
            "A2A_MESH_INBOUND_CAPACITY",
            "limits.inbound_capacity",
            inbound_capacity
        );
        if let Some(v) = lookup(env, "A2A_MESH_INBOUND_DEADLINE_SECS") {
            self.limits.inbound_deadline =
                Duration::from_secs(parse(v, "limits.inbound_deadline_secs")?);
        }
        if let Some(v) = lookup(env, "A2A_MESH_MAX_OUTBOUND_TRACKING_SECS") {
            self.limits.max_outbound_tracking =
                Duration::from_secs(parse(v, "limits.max_outbound_tracking_secs")?);
        }
        if let Some(v) = lookup(env, "A2A_MESH_TERMINAL_RETENTION_SECS") {
            self.limits.terminal_retention =
                Duration::from_secs(parse(v, "limits.terminal_retention_secs")?);
        }
        limit!(
            "A2A_MESH_TASKS_PER_PEER",
            "limits.tasks_per_peer",
            tasks_per_peer
        );
        limit!(
            "A2A_MESH_TASKS_PER_CONTEXT",
            "limits.tasks_per_context",
            tasks_per_context
        );
        limit!(
            "A2A_MESH_TOTAL_BLOB_BYTES",
            "limits.total_blob_bytes",
            total_blob_bytes
        );
        limit!(
            "A2A_MESH_INLINE_CONTENT_THRESHOLD",
            "limits.inline_content_threshold",
            inline_content_threshold
        );
        Ok(())
    }
    fn apply_cli(&mut self, args: &[String]) -> Result<(), ConfigError> {
        let mut i = 1;
        while i < args.len() {
            let name = &args[i];
            if name == "--config" {
                i += 2;
                continue;
            }
            let value = args
                .get(i + 1)
                .ok_or_else(|| ConfigError::MissingOptionValue(name.clone()))?;
            if value.starts_with("--") {
                return Err(ConfigError::MissingOptionValue(name.clone()));
            }
            match name.as_str() {
                "--etcd-endpoints" => self.etcd_endpoints = csv("etcd.endpoints", value)?,
                "--listen-address" => self.listen_address = parse_ip("listen.address", value)?,
                "--advertised-host" => self.advertised_host = Some(value.clone()),
                "--inbound-capacity" => {
                    self.limits.inbound_capacity = parse(value, "limits.inbound_capacity")?
                }
                "--inbound-deadline-secs" => {
                    self.limits.inbound_deadline =
                        Duration::from_secs(parse(value, "limits.inbound_deadline_secs")?)
                }
                "--max-outbound-tracking-secs" => {
                    self.limits.max_outbound_tracking =
                        Duration::from_secs(parse(value, "limits.max_outbound_tracking_secs")?)
                }
                "--terminal-retention-secs" => {
                    self.limits.terminal_retention =
                        Duration::from_secs(parse(value, "limits.terminal_retention_secs")?)
                }
                "--tasks-per-peer" => {
                    self.limits.tasks_per_peer = parse(value, "limits.tasks_per_peer")?
                }
                "--tasks-per-context" => {
                    self.limits.tasks_per_context = parse(value, "limits.tasks_per_context")?
                }
                "--total-blob-bytes" => {
                    self.limits.total_blob_bytes = parse(value, "limits.total_blob_bytes")?
                }
                "--inline-content-threshold" => {
                    self.limits.inline_content_threshold =
                        parse(value, "limits.inline_content_threshold")?
                }
                _ => return Err(ConfigError::UnknownOption(name.clone())),
            };
            i += 2;
        }
        Ok(())
    }
    /// Validate a configuration after programmatic construction or mutation.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.etcd_endpoints.is_empty() || self.etcd_endpoints.iter().any(|e| e.trim().is_empty())
        {
            return invalid(
                "etcd.endpoints",
                "at least one non-empty endpoint is required",
            );
        }
        for (field, value) in [
            ("limits.inbound_capacity", self.limits.inbound_capacity),
            ("limits.tasks_per_peer", self.limits.tasks_per_peer),
            ("limits.tasks_per_context", self.limits.tasks_per_context),
        ] {
            if value == 0 {
                return invalid(field, "must be greater than zero");
            }
        }
        for (field, value) in [
            (
                "limits.inbound_deadline_secs",
                self.limits.inbound_deadline.as_secs(),
            ),
            (
                "limits.max_outbound_tracking_secs",
                self.limits.max_outbound_tracking.as_secs(),
            ),
            (
                "limits.terminal_retention_secs",
                self.limits.terminal_retention.as_secs(),
            ),
            ("limits.total_blob_bytes", self.limits.total_blob_bytes),
            (
                "limits.inline_content_threshold",
                self.limits.inline_content_threshold,
            ),
        ] {
            if value == 0 {
                return invalid(field, "must be greater than zero");
            }
        }
        if self.limits.total_blob_bytes > usize::MAX as u64 {
            return invalid(
                "limits.total_blob_bytes",
                "exceeds this platform's addressable memory",
            );
        }
        for (field, value) in [
            (
                "limits.inbound_deadline_secs",
                self.limits.inbound_deadline.as_secs(),
            ),
            (
                "limits.max_outbound_tracking_secs",
                self.limits.max_outbound_tracking.as_secs(),
            ),
            (
                "limits.terminal_retention_secs",
                self.limits.terminal_retention.as_secs(),
            ),
        ] {
            if value > MAX_DURATION_SECS {
                return invalid(field, "must not exceed 365 days");
            }
        }
        if let Some(host) = &self.advertised_host {
            if host.trim().is_empty() || host.chars().any(char::is_control) {
                return invalid(
                    "listen.advertised_host",
                    "must be a non-blank host without control characters",
                );
            }
            url::Host::parse(host).map_err(|_| ConfigError::Invalid {
                field: "listen.advertised_host",
                message: format!("expected a DNS name or IP address, got {host:?}"),
            })?;
        }
        for endpoint in &self.etcd_endpoints {
            let parsed = url::Url::parse(endpoint).map_err(|_| ConfigError::Invalid {
                field: "etcd.endpoints",
                message: format!("expected an absolute http(s) URL, got {endpoint:?}"),
            })?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
                return invalid(
                    "etcd.endpoints",
                    "each endpoint must be an absolute http(s) URL with a host",
                );
            }
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return invalid(
                    "etcd.endpoints",
                    "endpoint URLs must not contain credentials",
                );
            }
        }
        if self.limits.inline_content_threshold > self.limits.total_blob_bytes {
            return invalid(
                "limits.inline_content_threshold",
                "cannot exceed limits.total_blob_bytes",
            );
        }
        Ok(())
    }
}
fn lookup<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
    env.iter()
        .rev()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}
fn parse<T: std::str::FromStr>(value: &str, field: &'static str) -> Result<T, ConfigError> {
    value.parse().map_err(|_| ConfigError::Invalid {
        field,
        message: format!("expected an unsigned integer, got {value:?}"),
    })
}
fn parse_ip(field: &'static str, value: &str) -> Result<IpAddr, ConfigError> {
    value.parse().map_err(|_| ConfigError::Invalid {
        field,
        message: format!("expected an IP address, got {value:?}"),
    })
}
fn csv(field: &'static str, value: &str) -> Result<Vec<String>, ConfigError> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if values.is_empty() {
        invalid(field, "at least one non-empty endpoint is required")
    } else {
        Ok(values)
    }
}
fn invalid<T>(field: &'static str, message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Invalid {
        field,
        message: message.into(),
    })
}
fn option_value(args: &[String], option: &str) -> Result<Option<String>, ConfigError> {
    let matches = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| *arg == option)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(ConfigError::Invalid {
            field: "config",
            message: format!("{option} may be specified only once"),
        });
    }
    match matches.first() {
        Some((index, _)) => match args.get(index + 1) {
            Some(value) if !value.starts_with("--") => Ok(Some(value.clone())),
            _ => Err(ConfigError::MissingOptionValue(option.into())),
        },
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn env(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }
    #[test]
    fn defaults_are_balanced() {
        let c = Config::load_from(["mesh"], &[]).unwrap();
        assert_eq!(c.limits.inbound_capacity, 64);
        assert_eq!(c.limits.inbound_deadline, Duration::from_secs(900));
        assert_eq!(c.limits.total_blob_bytes, 512 * MIB);
        assert_eq!(c.limits.inline_content_threshold, 256 * KIB);
    }
    #[test]
    fn environment_overrides_defaults() {
        let c = Config::load_from(
            ["mesh"],
            &env(&[
                ("A2A_MESH_INBOUND_CAPACITY", "7"),
                ("A2A_MESH_ETCD_ENDPOINTS", "http://a,http://b"),
            ]),
        )
        .unwrap();
        assert_eq!(c.limits.inbound_capacity, 7);
        assert_eq!(c.etcd_endpoints.len(), 2);
    }
    #[test]
    fn precedence_is_file_then_environment_then_cli() {
        let path = std::env::temp_dir().join(format!("a2a-mesh-config-{}", std::process::id()));
        std::fs::write(&path, "[limits]\ninbound_capacity = 5\n").unwrap();
        let path_text = path.to_string_lossy().into_owned();
        let config = Config::load_from(
            ["mesh", "--config", &path_text, "--inbound-capacity", "9"],
            &env(&[("A2A_MESH_INBOUND_CAPACITY", "7")]),
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(config.limits.inbound_capacity, 9);
    }
    #[test]
    fn cli_overrides_environment() {
        let c = Config::load_from(
            ["mesh", "--inbound-capacity", "9"],
            &env(&[("A2A_MESH_INBOUND_CAPACITY", "7")]),
        )
        .unwrap();
        assert_eq!(c.limits.inbound_capacity, 9);
    }
    #[test]
    fn every_limit_has_a_default_and_cli_override() {
        let defaults = Config::load_from(["mesh"], &[]).unwrap();
        assert_eq!(
            (
                defaults.limits.inbound_capacity,
                defaults.limits.inbound_deadline.as_secs(),
                defaults.limits.max_outbound_tracking.as_secs(),
                defaults.limits.terminal_retention.as_secs(),
                defaults.limits.tasks_per_peer,
                defaults.limits.tasks_per_context,
                defaults.limits.total_blob_bytes,
                defaults.limits.inline_content_threshold
            ),
            (64, 900, 86400, 86400, 256, 256, 512 * MIB, 256 * KIB)
        );
        let configured = Config::load_from(
            [
                "mesh",
                "--inbound-capacity",
                "1",
                "--inbound-deadline-secs",
                "2",
                "--max-outbound-tracking-secs",
                "3",
                "--terminal-retention-secs",
                "4",
                "--tasks-per-peer",
                "5",
                "--tasks-per-context",
                "6",
                "--total-blob-bytes",
                "8",
                "--inline-content-threshold",
                "7",
            ],
            &[],
        )
        .unwrap();
        assert_eq!(
            (
                configured.limits.inbound_capacity,
                configured.limits.inbound_deadline.as_secs(),
                configured.limits.max_outbound_tracking.as_secs(),
                configured.limits.terminal_retention.as_secs(),
                configured.limits.tasks_per_peer,
                configured.limits.tasks_per_context,
                configured.limits.total_blob_bytes,
                configured.limits.inline_content_threshold
            ),
            (1, 2, 3, 4, 5, 6, 8, 7)
        );
    }
    #[test]
    fn rejects_unknown_or_unusable_input() {
        let path = std::env::temp_dir().join(format!("a2a-mesh-unknown-{}", std::process::id()));
        std::fs::write(&path, "[limits]\ninbound_capcity = 5\n").unwrap();
        let path_text = path.to_string_lossy().into_owned();
        assert!(Config::load_from(["mesh", "--config", &path_text], &[]).is_err());
        std::fs::remove_file(path).unwrap();
        for args in [
            ["mesh", "--advertised-host", ""].as_slice(),
            ["mesh", "--etcd-endpoints", "not-a-url"].as_slice(),
            [
                "mesh",
                "--etcd-endpoints",
                "http://user:password@localhost:2379",
            ]
            .as_slice(),
            ["mesh", "--advertised-host", "https://example.test/path"].as_slice(),
            ["mesh", "--advertised-host", "--password"].as_slice(),
            ["mesh", "--config", "/dev/null", "--config", "/dev/null"].as_slice(),
        ] {
            assert!(Config::load_from(args.iter().copied(), &[]).is_err());
        }
    }
    #[test]
    fn rejects_zero_and_contradictory_limits() {
        let error = Config::load_from(["mesh", "--inbound-capacity", "0"], &[]).unwrap_err();
        assert!(error.to_string().contains("limits.inbound_capacity"));
        let error = Config::load_from(
            [
                "mesh",
                "--total-blob-bytes",
                "1",
                "--inline-content-threshold",
                "2",
            ],
            &[],
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("limits.inline_content_threshold"));
    }
    #[test]
    fn rejects_unknown_options_so_secrets_are_not_silently_accepted() {
        assert!(matches!(
            Config::load_from(["mesh", "--password", "secret"], &[]),
            Err(ConfigError::UnknownOption(_))
        ));
    }
}
