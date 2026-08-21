# Resources and discovery DTOs

Resources use a stable descriptor. `uri` and `name` are required strings;
`description` and `media_type` are nullable strings.

```json
{"uri":"mesh://peers","name":"Peers","description":"The current discovery snapshot","media_type":"application/json"}
```

## Registration

Lease-bound registrations use schema version `1`. `instance_id` is the process
UUID and `handle` is its mesh-visible name. `endpoint` is the advertised peer
URL. `interface` declares the JSON-RPC protocol and version. The lease-bound,
monotonically increasing `registration_version` changes when the agent card
changes. `process` contains nullable observed `cwd`, `repository`, `branch`,
and `pid` metadata.

```json
{
  "schema_version":1,
  "instance_id":"6f0d9b1b-f4c3-4e5e-9a8d-7f9f559898a4",
  "handle":"planner@laptop",
  "endpoint":"https://127.0.0.1:4567",
  "interface":{"protocol":"jsonrpc","version":"1.0"},
  "registration_version":4,
  "process":{"cwd":"/work/project","repository":null,"branch":"trunk","pid":42},
  "security":{"mode":"pinned-tls","spki_sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","certificate_expires_at":"2027-01-01T00:00:00Z"}
}
```

`security.mode` is `plaintext` or `pinned-tls`. `spki_sha256` is the lowercase
64-character hexadecimal SHA-256 SPKI fingerprint. `certificate_expires_at` is
the certificate's RFC 3339 expiration timestamp.

The companion versioned handle claim identifies the owner of its separate
handle key:

```json
{"schema_version":1,"instance_id":"6f0d9b1b-f4c3-4e5e-9a8d-7f9f559898a4","handle":"planner@laptop"}
```

## Peer and identity status

A peer retains stale data visibly rather than omitting it. `identity_verified`
is true only for a sender matching a registered mesh identity. An unverified
conforming sender has `identity_verified: false` and may have null identity
fields. `reachable` is a local connection overlay. Snapshot `health` is one of
`fresh`, `stale`, or `unavailable`; `revision` is the discovery revision,
`last_successful_refresh` is a nullable RFC 3339 time, and `stale_age_ms` is a
nullable age in milliseconds.

```json
{
  "identity":{"instance_id":null,"handle":null,"identity_verified":false},
  "endpoint":"https://127.0.0.1:4567",
  "reachable":false,
  "snapshot":{"health":"stale","revision":91,"last_successful_refresh":"2026-08-21T18:00:00Z","stale_age_ms":3500},
  "registration":{"schema_version":1,"instance_id":"6f0d9b1b-f4c3-4e5e-9a8d-7f9f559898a4","handle":"planner@laptop","endpoint":"https://127.0.0.1:4567","interface":{"protocol":"jsonrpc","version":"1.0"},"registration_version":4,"process":{"cwd":null,"repository":null,"branch":null,"pid":null},"security":{"mode":"pinned-tls","spki_sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","certificate_expires_at":"2027-01-01T00:00:00Z"}}
}
```

A verified peer has populated mesh identity fields:

```json
{"identity":{"instance_id":"6f0d9b1b-f4c3-4e5e-9a8d-7f9f559898a4","handle":"planner@laptop","identity_verified":true}}
```
