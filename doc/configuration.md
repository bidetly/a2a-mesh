# Configuration

`a2a-mesh` builds one typed configuration before it starts listeners or background tasks. It contains listener and etcd security policy, but never certificate keys, credentials, or authentication tokens.

## Sources and precedence

Values are merged in this order, with later sources overriding earlier ones:

1. TOML file selected by `--config PATH`, or `A2A_MESH_CONFIG` when the option is absent.
2. Environment variables.
3. Command-line options.

An absent configuration file selector does not cause a file to be read. A selected unreadable or invalid file is an error. Unknown command-line options are errors, preventing secrets from being accidentally accepted as ordinary settings.

## Fields

| TOML field | Environment | Command line | Default | Unit / validation |
| --- | --- | --- | --- | --- |
| `etcd.endpoints` | `A2A_MESH_ETCD_ENDPOINTS` | `--etcd-endpoints` | `http://127.0.0.1:2379` | one or more absolute `http`/`https` URLs with hosts and no URL credentials; environment/CLI use comma-separated endpoints |
| `listen.address` | `A2A_MESH_LISTEN_ADDRESS` | `--listen-address` | `127.0.0.1` | IP address |
| `listen.advertised_host` | `A2A_MESH_ADVERTISED_HOST` | `--advertised-host` | unset | non-blank DNS name or IP address, with no control characters |
| `security.a2a_tls` | `A2A_MESH_A2A_TLS` | `--a2a-tls` | false on loopback | boolean; required to be `true` for non-loopback or wildcard A2A binds |
| `security.present_client_certificate` | `A2A_MESH_PRESENT_CLIENT_CERTIFICATE` | `--present-client-certificate` | true | boolean; generated identity is presented outbound when enabled |
| `security.certificate_lifetime_secs` | `A2A_MESH_CERTIFICATE_LIFETIME_SECS` | `--certificate-lifetime-secs` | 31622400 | seconds; positive, no more than 10 years; certificate expiration is this lifetime plus a five-minute clock-skew margin |
| `security.allow_insecure_etcd` | `A2A_MESH_ALLOW_INSECURE_ETCD` | `--allow-insecure-etcd` | false | boolean; explicit opt-in required for non-loopback plaintext etcd and emits one startup warning |
| `limits.inbound_capacity` | `A2A_MESH_INBOUND_CAPACITY` | `--inbound-capacity` | 64 | tasks; positive integer |
| `limits.inbound_deadline_secs` | `A2A_MESH_INBOUND_DEADLINE_SECS` | `--inbound-deadline-secs` | 900 | seconds (15 minutes); positive integer, at most 365 days |
| `limits.max_outbound_tracking_secs` | `A2A_MESH_MAX_OUTBOUND_TRACKING_SECS` | `--max-outbound-tracking-secs` | 86400 | seconds (24 hours); positive integer, at most 365 days |
| `limits.terminal_retention_secs` | `A2A_MESH_TERMINAL_RETENTION_SECS` | `--terminal-retention-secs` | 86400 | seconds (24 hours); positive integer, at most 365 days |
| `limits.tasks_per_peer` | `A2A_MESH_TASKS_PER_PEER` | `--tasks-per-peer` | 256 | tasks; positive integer |
| `limits.tasks_per_context` | `A2A_MESH_TASKS_PER_CONTEXT` | `--tasks-per-context` | 256 | tasks; positive integer |
| `limits.total_blob_bytes` | `A2A_MESH_TOTAL_BLOB_BYTES` | `--total-blob-bytes` | 536870912 | bytes (512 MiB); positive integer, no greater than the platform address-space limit |
| `limits.inline_content_threshold` | `A2A_MESH_INLINE_CONTENT_THRESHOLD` | `--inline-content-threshold` | 262144 | bytes (256 KiB); positive integer no greater than `total_blob_bytes` |

A malformed number, address, TOML document, empty endpoint list, zero resource/duration limit, a duration over 365 days, blob size over the platform address-space limit, or inline threshold greater than total blob storage fails startup with the offending field named.

## Example

```toml
[etcd]
endpoints = ["https://etcd-a:2379", "https://etcd-b:2379"]

[listen]
address = "0.0.0.0"
advertised_host = "mesh-worker.internal"

[security]
a2a_tls = true
present_client_certificate = true
certificate_lifetime_secs = 31622400

[limits]
inbound_capacity = 128
inbound_deadline_secs = 600
total_blob_bytes = 1073741824
inline_content_threshold = 262144
```

## Dependency and lockfile policy

`Cargo.lock` is part of this binary crate's reproducible build configuration. CI builds and tests with `--locked`; dependency declaration changes must include the generated lockfile update. Run:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --locked
cargo test --locked
```

See [security.md](security.md) for the TLS identity, fingerprint, pinning, non-mesh trust, certificate lifetime, etcd, and safe-secret-handling model.
