# Tools

`a2a-mesh` tool results use documented JSON DTOs. Field names are stable;
unknown fields and undocumented aliases are rejected when DTOs are decoded.

## Tool descriptors

A tool descriptor has `name` (string), optional `description` (string or
`null`), and `input_schema` (a JSON Schema value):

```json
{"name":"mesh_send","description":"Send work to a peer","input_schema":{"type":"object"}}
```

## Task result

A task result distinguishes the local opaque mesh identifier from the ID supplied
by an A2A peer:

```json
{
  "task_id": "mesh-opaque-2a4c",
  "a2a_task_id": "peer-task-19",
  "context_id": "review-7",
  "state": "working",
  "parts": [{"kind":"text","text":"Started","media_type":"text/plain","filename":null,"metadata":{}}],
  "artifacts": []
}
```

`task_id` is a process-generated opaque string accepted by mesh task operations.
`a2a_task_id` and `context_id` are nullable peer/context strings. `parts` and
`artifacts` are arrays of the DTOs below. States are exactly `submitted`,
`working`, `input-required`, `auth-required`, `completed`, `failed`,
`canceled`, or `rejected`; consumers must not collapse them.

## Content and artifacts

`ContentPart` is a tagged one-of: `kind` is `text`, `raw`, `url`, or `data`,
and selects exactly one matching payload field. `raw` is base64-encoded binary;
`data` is an arbitrary JSON value. All variants carry optional `media_type` and
`filename`, plus `metadata` (an object).

```json
{"kind":"text","text":"started","media_type":"text/plain","filename":null,"metadata":{}}
{"kind":"raw","raw":"aGVsbG8=","media_type":"application/octet-stream","filename":"hello.bin","metadata":{}}
{"kind":"url","url":"mesh://blob/1","media_type":null,"filename":null,"metadata":{}}
{"kind":"data","data":{"accepted":true},"media_type":"application/json","filename":null,"metadata":{"source":"peer"}}
```

An artifact retains its identity and stream controls. `name` and `description`
are nullable strings; `parts` is an array; `append` and `lastChunk` are
booleans; `metadata` is an object.

```json
{"artifact_id":"artifact-1","name":"summary.json","description":null,"parts":[{"kind":"text","text":"done","media_type":"text/plain","filename":null,"metadata":{}}],"append":false,"lastChunk":true,"metadata":{}}
```

## Identity handles and process metadata

A mesh handle is a **canonical**, ASCII-only `local@host` string. The parser
never trims, lowercases, percent-decodes, or Unicode-normalizes input; alternate
spellings are rejected rather than silently mapped to another identity.

- There is exactly one `@`.
- `local` is 1–63 bytes. It consists of lowercase ASCII letters, digits, and
  hyphen-separated labels; each label is 1–63 bytes and starts and ends with a
  letter or digit.
- `host` is 1–253 bytes. It consists of lowercase ASCII letters, digits,
  hyphens, and dot-separated labels; each label is 1–63 bytes and starts and
  ends with a letter or digit.
- The entire handle is at most 317 bytes.

Valid: `planner@dev-laptop`, `build-2@ci.example.test`, `a@a`.

Invalid: `Planner@dev-laptop` (uppercase), `planner@` (empty host),
`planner@@host` (ambiguous separator), `planner@host..test` (empty label),
`planner%2f@host`, `planner@höst`, and `planner @host`.

Handle-claim key segments are base64url **without padding** of the canonical
handle UTF-8 bytes. Decoding validates the UTF-8 value as a canonical handle
again. Therefore a segment is reversible and uses only `A-Z`, `a-z`, `0-9`,
`-`, and `_`; it cannot include `/`, `%`, or escape a discovery key prefix.

Identity tools collect process metadata best-effort. `Registration.instance_id`
is one UUID allocated for the process. `process.pid`, `process.cwd`,
`process.hostname`, `process.repository`, and `process.branch` are optional.
Missing, unsafe, unavailable, detached-head, or non-UTF-8 probe results are
omitted and never prevent startup or announcement. Repository identity is only
reported for recognized credential-free GitHub origins, normalized to
`github.com/owner/repository`; raw remote URLs, userinfo, queries, fragments,
and environment-derived metadata are never returned.
