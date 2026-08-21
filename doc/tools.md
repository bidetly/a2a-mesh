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
