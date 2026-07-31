# a2a-mesh

**A Rust MCP server for agent-to-agent messaging over A2A**

Target: A2A Protocol v1.0.0 · MCP specification 2026-07-28

---

## 1. What this is

A single binary, `a2a-mesh`, run as an MCP server by a coding agent such as
Claude Code. It lets that agent find other agents running nearby and exchange
work with them over the A2A protocol.

Each instance is three things in one process:

- an **MCP server** for the one agent that spawned it,
- an **A2A server** accepting tasks from other agents,
- an **A2A client** sending tasks to other agents.

Discovery runs through **etcd**. Each instance writes its agent card to an etcd
key held by a lease and reads the key prefix to see peers.

```
  Claude Code (repo A)                      Claude Code (repo B)
        │ MCP stdio                                │ MCP stdio
        ▼                                          ▼
  ┌─────────────┐ ◄────────── A2A ──────────► ┌─────────────┐
  │  a2a-mesh   │                             │  a2a-mesh   │
  └──────┬──────┘                             └──────┬──────┘
         │  put(card, lease)  watch(prefix)          │
         └──────────────► etcd ◄────────────────────-┘
```

Everything on the wire between instances is A2A v1.0. Any conformant A2A agent
can be talked to and can talk to us. Only discovery is specific to this project,
and discovery is a lookup rather than a protocol.

### 1.1 Assumptions

- **The mesh is trusted.** All participants are operated by the same person or
  team. There is no adversarial peer in the threat model.
- **Multiple instances run on one machine.** Ports are assigned dynamically and
  identity distinguishes instances by working directory (§4).
- **State is disposable.** If an instance dies, its in-flight work dies with it.
  Nothing is written to disk (§7).

### 1.2 Out of scope

**Coordinating two agents working in the same directory.** Working directory
appears in peer metadata so agents can see the overlap, but this system provides
no locking, no leases on paths, and no conflict detection. Two agents in one
tree coordinate that between themselves — possibly by messaging each other
through this system, which is an ordinary use of it and not a feature of it.

---

## 2. Crate layout

One crate.

```
a2a-mesh/
├── Cargo.toml
└── src/
    ├── main.rs         # config, wiring, tokio setup
    ├── identity.rs     # handle, card construction, announce state
    ├── discovery.rs    # etcd: lease, put, cached prefix reads
    ├── inbound.rs      # A2A server: AgentExecutor impl, message queue
    ├── outbound.rs     # A2A client: send, task tracking
    ├── bridge.rs       # A2A task <-> MCP task
    ├── tools.rs        # MCP tool definitions
    └── error.rs
```

Splitting into a workspace is a later decision, to be made when a boundary is
proven rather than guessed at.

### 2.1 Dependencies

| Crate | Version | Role |
|---|---|---|
| `rmcp` | `3` | MCP server, `task_manager`, tool macros |
| `a2a-lf` | `0.3` | A2A core types |
| `a2a-client-lf` | `0.2` | A2A client, binding negotiation |
| `a2a-server-lf` | `0.4` | A2A server, `axum`-based |
| `etcdrs` | `0.1` | etcd v3 client |
| `etcdrs-util` | `0.1` | `CacheClient`, `LeasePool` |
| `tokio` | `1` | Runtime |
| `axum` | `0.8` | HTTP, shared with `a2a-server-lf` |
| `serde` / `serde_json` | `1` | |
| `thiserror` | `2` | Errors |
| `tracing` / `tracing-subscriber` | `0.1` / `0.3` | Logs |
| `uuid` | `1` | Identifiers |

The `-lf` A2A crates are published on crates.io; they are pre-1.0, so a minor
bump is a breaking change and Cargo holds the pin until it is raised
deliberately.

`rmcp` needs features `server`, `macros`, `schemars`, `transport-io`.
`etcdrs-util` needs `cache` and `lease-pool`, both on by default.
`a2a-server-lf` and `a2a-client-lf` both use `axum` 0.8 and `reqwest` 0.13, so
one HTTP stack serves both directions.

---

## 3. MCP tool surface

Every tool returns a JSON object as `structuredContent`, with a short text
rendering alongside it for models that read text more reliably than schemas.
Schemas below describe the structured form.

### 3.1 Identity

**`mesh_announce`** — the agent declares what it is and what it can do.
**Announcement is required.** Until it is called, the instance holds no etcd key
and is invisible to peers. There is no generic startup card: a card that says
"an agent, somewhere, possibly useful" invites messages the agent cannot answer.
The agent is expected to look at what it has been asked to do and describe
itself deliberately.

```jsonc
// args
{ "name": "payments-refactor",
  "description": "Working on the payments service in acme/backend. Can read, modify, and test Rust code in this repo.",
  "skills": [
    { "id": "rust-refactor",
      "name": "Rust refactoring",
      "description": "Restructure Rust code in the acme/backend payments module.",
      "tags": ["rust", "payments"] } ] }

// returns
{ "handle": "payments-refactor@dev-laptop",   // may differ from requested name
  "endpoint": "http://127.0.0.1:53411",
  "card_version": 1,
  "renamed": false }                          // true if a discriminator was appended
```

The agent knows what it is working on; the process does not. A coding agent in a
repository can describe its own competence, and can call `mesh_announce` again
when that changes — it moves to a different part of the tree, or finishes a task
and becomes free for something else. Each call replaces the card and increments
`card_version`.

The node fills in the mechanical fields itself: handle, endpoint URL,
capabilities, working directory, git remote and branch. Those are observable.
Skill claims are not, so the agent supplies them.

Every peer-facing tool returns `NotAnnounced` before the first successful
announce, with an error saying to call `mesh_announce` first.

**`mesh_whoami`**
```jsonc
{ "announced": true,
  "handle": "payments-refactor@dev-laptop",
  "endpoint": "http://127.0.0.1:53411",
  "card_version": 1,
  "card": { /* full agent card as peers see it */ },
  "etcd": { "connected": true, "lease_ttl_secs": 30 } }
```

### 3.2 Discovery

**`mesh_list_peers`**
```jsonc
// args
{ "skill": "string?", "limit": "u32?" }

// returns
{ "peers": [
    { "handle": "docs-writer@dev-laptop",
      "name": "Docs writer",
      "description": "...",
      "skills": [ { "id": "...", "name": "...", "tags": ["..."] } ],
      "cwd": "/home/me/src/acme-docs",
      "repo": "git@github.com:acme/docs.git",
      "branch": "main",
      "endpoint": "http://127.0.0.1:53987",
      "reachable": true } ],   // false once a send has failed; see §5.6
  "self_excluded": true }
```

`mesh_list_peers` never lists the calling instance.

**`mesh_describe_peer`** — `{ "handle": "..." }` returns the peer's full agent
card plus the same mesh metadata.

Peer records carry working directory and repository because those are the most
useful disambiguators here. "Which of these four agents is in the repo I care
about" is the common question.

### 3.3 Sending

**`mesh_send`**
```jsonc
// args
{ "to": "docs-writer@dev-laptop",
  "parts": [ { "text": "Does the payments module still export `Charge`?" } ],
  "context_id": "string?",
  "reference_task_ids": ["..."],
  "timeout_ms": "u32?" }        // default 10000
```

The node waits up to `timeout_ms`. What comes back is one of three shapes,
discriminated by `outcome`.

**`outcome: "message"`** — the peer replied without creating a task. Nothing to
track; the exchange is over.
```jsonc
{ "outcome": "message",
  "context_id": "ctx-abc",
  "parts": [ { "text": "Yes, still exported from payments::charge." } ] }
```

**`outcome: "completed"`** — the peer created a task and it reached a terminal
state inside the window. The result is inline; there is still a `task_id`
because the peer may hold artifacts worth referencing later.
```jsonc
{ "outcome": "completed",
  "task_id": "task-9f2",
  "context_id": "ctx-abc",
  "state": "completed",              // or "failed" | "canceled" | "rejected"
  "parts": [ { "text": "..." } ],
  "artifacts": [
    { "artifact_id": "art-1",
      "name": "summary.md",
      "parts": [ { "text": "..." } ] } ] }
```

**`outcome: "pending"`** — the peer created a task still running when the window
closed. Tracking continues in the background.
```jsonc
{ "outcome": "pending",
  "task_id": "task-9f2",
  "context_id": "ctx-abc",
  "state": "working",                // or "submitted" | "input-required"
  "mcp_task_id": "mcp-task-4c1",     // for tasks/get, tasks/cancel
  "waited_ms": 10000,
  "next": "Call mesh_get_task with task_id \"task-9f2\", or wait for the task to complete." }
```

Three properties of this shape:

- **`outcome` is a discriminator, not a status string.** A model branching on
  three named cases is more reliable than one inferring intent from which
  fields happen to be present.
- **`task_id` and `context_id` appear wherever they exist**, so a follow-up call
  can always find them in the same place.
- **`pending` carries `next`.** The agent that just received a handle needs to
  know what to do with it, and a sentence costs less than a failed guess.

`state` carries the A2A state verbatim, so the agent sees what the peer actually
reported.

**`mesh_get_task`** — `{ "task_id": "..." }`
```jsonc
{ "task_id": "task-9f2",
  "context_id": "ctx-abc",
  "state": "completed",
  "direction": "outbound",           // or "inbound"
  "peer": "docs-writer@dev-laptop",
  "parts": [ ... ],                  // latest message parts
  "artifacts": [ ... ],
  "history": [ { "state": "submitted", "at": "..." },
               { "state": "working",   "at": "..." },
               { "state": "completed", "at": "..." } ] }
```

**`mesh_cancel_task`** — `{ "task_id": "..." }` → `{ "task_id": "...", "state": "canceled" }`.
Cancellation is cooperative; if the peer declines, `state` reflects what the
peer reported.

**`mesh_list_tasks`** — `{ "direction": "...?", "state": "...?", "peer": "...?" }`
→ `{ "tasks": [ { "task_id", "context_id", "peer", "direction", "state", "updated_at" } ] }`.

**`mesh_respond`** — supply input to a task in `input-required`, either
direction. `{ "task_id": "...", "parts": [...] }` → the same three-outcome shape
as `mesh_send`, since responding can equally resolve or continue the task.

### 3.4 Receiving

**`mesh_inbox`** — `{ "limit": "u32?" }`
```jsonc
{ "messages": [
    { "task_id": "task-77a",
      "context_id": "ctx-xyz",
      "from": "planner@dev-laptop",
      "parts": [ { "text": "Can you check whether `Charge` is still exported?" } ],
      "received_at": "...",
      "expires_at": "...",           // deadline from §6.3
      "state": "submitted",          // or "input-required" if we asked and they answered
      "is_continuation": false } ],  // true if context_id matches a task we've seen
  "queue": { "depth": 1, "capacity": 64 } }
```

`is_continuation` and `context_id` let an agent tell a new request from a reply
in a conversation it started earlier. The full history is at `mesh://task/{id}`.

**`mesh_reply`**
```jsonc
// args
{ "task_id": "task-77a",
  "parts": [ { "text": "Yes, from payments::charge." } ],
  "final": true,          // default true: completes the task
  "artifacts": [ ... ] }  // optional

// returns
{ "task_id": "task-77a", "state": "completed" }
```

`final: false` moves the task to `input-required`, which is how the agent asks
the sender a question in place of answering.

### 3.5 Resources

- `mesh://peers` — the peer list.
- `mesh://inbox` — pending inbound messages. Resource-updated notifications fire
  on this URI when a message arrives.
- `mesh://task/{id}` — a task's full transcript.
- `mesh://blob/{id}` — a large artifact payload (§8.2).

---

## 4. Identity

### 4.1 Handles

`local-name@host`. The local name comes from `mesh_announce`; the host part is
the machine's hostname. `payments-refactor@dev-laptop`.

Multiple instances per machine is normal, so collisions are expected. If
`payments-refactor@dev-laptop` is already held by a live peer, the node appends
a discriminator — `payments-refactor-2@dev-laptop` — and returns the actual
handle with `renamed: true`. The agent is told what it ended up being called.

```rust
pub struct Handle { pub local: String, pub host: String }
```

The etcd key is a process-unique UUID rather than the handle, so a rename
through re-announcing does not orphan a key.

### 4.2 Card construction

| Field | Source |
|---|---|
| `name`, `description`, `skills` | `mesh_announce` arguments |
| `url` | Bound listener address (§4.3) |
| `capabilities` | What the node has enabled: `streaming: true`, `pushNotifications: false` |
| `version` | Incremented on each announce |
| `metadata.cwd`, `.repo`, `.branch`, `.pid` | Observed from the process environment |

`a2a-server-lf` exposes `AgentCardProducer` as a trait, so the card served at
`/.well-known/agent-card.json` is generated per request from current state.
Re-announcing changes what peers fetch immediately.

Before the first announce, the A2A listener is bound but the card endpoint
returns 404. An unannounced instance is not an agent yet.

### 4.3 Ports

The A2A listener binds to port 0 and takes whatever the OS assigns. The actual
address goes into the card and into etcd. Multiple instances on a machine never
conflict and no port range needs managing.

Bind address defaults to `127.0.0.1`; configurable to `0.0.0.0` when the mesh
spans hosts.

---

## 5. Discovery via etcd

### 5.1 Connecting

`etcdrs::Client` is built from a comma-separated connection string:

```rust
let client = Client::builder()
    .connection_string(&config.etcd.connection_string)?
    .build()?;
```

One endpoint connects lazily; several are balanced across. Lazy connection means
`build` succeeds without etcd being up, which is why an unreachable etcd at
startup is not fatal (§5.5).

`configure_endpoint` is available for connect timeouts and TLS if the mesh grows
beyond a single machine; `credentials` and `auth_token` cover an authenticated
cluster. None of these are needed for the loopback default.

### 5.2 Keys

```
/a2a-mesh/agents/{uuid}   →  { handle, card, endpoint, cwd, repo, branch, pid }
```

One key per instance. The value is the serialized agent card plus mesh metadata.
The key is written by `mesh_announce` and by nothing else.

### 5.3 Registration and lease

`etcdrs-util` provides `LeasePool`, which grants leases, groups them by TTL, and
keeps them alive in a background task:

```rust
let pool = LeasePool::new(client.clone());
let lease_id = pool.get_lease(Duration::from_secs(30)).await?;
client.put(key).value(card_json).lease(lease_id).await?;
```

The pool owns renewal. There is no deregistration path — the key exists exactly
as long as the lease is renewed.

Re-announcing overwrites the value at the same key under the same lease.

### 5.4 Reading peers

Reads dominate this workload: `mesh_list_peers` and handle resolution happen
constantly, registrations rarely. `etcdrs-util`'s `CacheClient` fits that shape.
It serves reads of configured key ranges from a local in-memory store kept
coherent by a watch, and passes through to the server for anything it cannot
prove fresh. Watch maintenance, cache coherence, and reconnection are its
responsibility.

```rust
let cache = CacheClient::builder(client)
    .cache_prefix("/a2a-mesh/agents/")
    .build();

// Served locally once warm.
let peers = cache.list_prefix("/a2a-mesh/agents/").await?;
```

Properties the design relies on:

- Construction does not block or wait for warmup. Reads pass through until the
  initial seed completes, so startup has no ordering constraint.
- A read through the cache is never slower than one through the plain client.
- Writes always go to the server, and the write's revision gates subsequent
  cached reads, so an announce is never followed by a stale self-read.
- Handles are cheap to clone; dropping the last one stops the watch.

### 5.5 Configuration

```toml
[etcd]
connection_string = "http://127.0.0.1:2379"
prefix            = "/a2a-mesh"
lease_ttl         = "30s"

[listen]
address = "127.0.0.1"

[inbound]
capacity = 64
deadline = "15m"
```

Multiple endpoints are comma-separated in the same string:
`"http://a:2379,http://b:2379"`. The value may also come from
`ETCD_CONNECTION_STRING`.

If etcd is unreachable at startup the node still serves MCP and reports peers as
unavailable, so an agent can work without the mesh and pick it up when etcd
arrives.

The 30 second lease TTL bounds how long a dead peer stays visible (§5.6).
Shorter means faster detection and more etcd traffic; `LeasePool` groups leases
by TTL, which keeps that traffic proportional to the number of distinct TTLs
rather than the number of keys.

### 5.6 How a peer death is detected

Three separate mechanisms, covering three different failures. No single one of
them is sufficient.

**A process that dies stops renewing its lease.** `LeasePool` renews in a
background task inside the process. If the process exits, crashes, is killed, or
is suspended past the TTL, renewal stops, etcd expires the lease, and every key
held by that lease is deleted. Other nodes see the deletion as a watch event
through `CacheClient` and drop the peer from their local view. Detection latency
is bounded by the lease TTL — up to 30 seconds by default.

This covers the common case: a session ends, the terminal closes, the laptop
sleeps.

**A peer that is gone but not expired fails at send time.** Within the TTL
window a dead peer is still listed. `mesh_send` to it fails on connection
refused. The node reports `PeerUnreachable` and sets `reachable: false` in its
local view so `mesh_list_peers` flags it before the lease expires. It does
**not** delete the etcd key: that key belongs to the peer's lease, and a node
deleting another node's registration would be guessing.

This covers the window between death and expiry.

**A peer that is alive but not answering is a task-level problem, not a liveness
problem.** A process that is running, holding its lease, and accepting
connections, but whose agent never reads its inbox, is not dead. Its tasks time
out (§6.3) and the sender sees a failed task with a reason. The peer is there
and may answer the next message.

The three differ in what they prove: lease expiry is authoritative and slow,
connection failure is immediate and local, task timeout is about attention
rather than existence. A node reports which one it observed rather than
collapsing them into "peer is down."

**On our own liveness:** if this node's lease expires because etcd became
unreachable while the process kept running, `LeasePool` re-establishes when
connectivity returns, and the node re-puts its key from the card held in memory.

---

## 6. Inbound messages

The requirement: the agent needs to know a message arrived, and needs to be able
to read it.

### 6.1 Mechanism

1. A peer sends an A2A message. `a2a-server-lf` routes it to our `AgentExecutor`
   implementation.
2. The executor puts the message on an in-memory queue and returns a task in
   `submitted` state. The peer now has a task id it can poll or subscribe to.
3. The node emits an MCP resource-updated notification for `mesh://inbox`.
4. The agent calls `mesh_inbox`, reads the message, and calls `mesh_reply`.
5. `mesh_reply` drives the A2A task to `working` and then to a terminal state,
   or to `input-required` when `final` is false.

Step 3 is best-effort: whether an MCP client surfaces a notification to its
agent varies by client. `mesh_inbox` works regardless, so an agent that checks
its inbox at the start of a turn never depends on notification support.

### 6.2 Queue

Bounded, in memory, default capacity 64. When full, further inbound messages are
rejected with an A2A error saying the agent is saturated. Refusing work tells
the sender something it can act on; silently accumulating messages nobody will
read does not.

### 6.3 Timeouts

A message unread past `inbound.deadline` (default 15 minutes) is failed with a
reason saying the agent did not pick it up, so a peer is never blocked
indefinitely on an agent that has stopped working.

### 6.4 Task lifecycle

```
  message arrives → submitted
                       │ mesh_reply final:false → input-required ⇄ peer responds
                       │ mesh_reply final:true  → completed
                       │ deadline / queue full  → failed
```

A2A tasks are immutable once terminal. A follow-up is a new task in the same
`contextId`; `mesh_reply` against a terminal task returns an error saying so and
naming the context id to use.

---

## 7. State

Nothing is written to disk.

| State | Where |
|---|---|
| Inbound tasks | `a2a-server-lf`'s `InMemoryTaskStore` |
| Outbound task tracking | In-memory map |
| Peer table | `CacheClient`'s in-memory store, watch-maintained |
| Agent card | In-memory, from `mesh_announce` |
| etcd lease | Held by `LeasePool`, expires with the process |

If the process dies, in-flight work is lost, the lease expires, peers see it
vanish, and the agent announces again on restart. This suits a process whose
lifetime is tied to an interactive coding session.

`a2a-server-lf` provides a `TaskStore` trait with `InMemoryTaskStore` as the
supplied implementation, so a durable store can be substituted behind the same
trait.

---

## 8. The task bridge

MCP's Tasks extension (SEP-2663) is implemented in `rmcp` 3.x as
`rmcp::task_manager`. Its state model corresponds closely to A2A's:

| A2A `TaskState` | MCP task state |
|---|---|
| `submitted`, `working` | `working` |
| `input-required`, `auth-required` | `input_required` |
| `completed` | terminal, `result` |
| `failed`, `canceled`, `rejected` | terminal, `error` (distinguished by code) |

### 8.1 Outbound

`mesh_send` calls A2A `SendMessage` and waits up to `timeout_ms`. A `Message`
response, or a `Task` reaching terminal state inside the window, returns
`outcome: "message"` or `outcome: "completed"` (§3.3).

Otherwise the node calls `TaskManager::spawn` and returns `outcome: "pending"`
carrying both the A2A `task_id` and the MCP `mcp_task_id`. `rmcp` guarantees the
task is visible to `tasks/get` before `spawn` returns, so the handle is never
dangling.

The spawned future consumes A2A updates and republishes them as MCP task state.
Transport preference: SSE via `SendStreamingMessage` when the peer's card
advertises `streaming`, otherwise polling with backoff (1s, 2s, 5s, 15s, then
every 15s).

Push notifications are not implemented. They require an externally reachable
webhook, which conflicts with binding to a loopback ephemeral port.

### 8.2 Content mapping

A2A `Part` is a `oneof` over `text` / `raw` / `url` / `data`, with `mediaType`,
`filename`, and `metadata`.

| A2A Part | MCP content |
|---|---|
| `text` | text |
| `raw` + image mediaType | image |
| `raw` + audio mediaType | audio |
| `raw` + other | embedded resource |
| `url` | resource link, URI preserved |
| `data` | text (formatted JSON) plus `structuredContent` |

`mediaType` and `filename` survive both directions. Artifacts arriving in
`TaskArtifactUpdateEvent` chunks are accumulated by `artifactId` using the
`append` and `lastChunk` flags.

`raw` payloads above 256 KiB are held in memory and exposed as a
`mesh://blob/{id}` resource, so a large artifact enters the agent's context only
when it reads that resource.

---

## 9. Errors

```rust
pub enum MeshError {
    NotAnnounced,
    UnknownPeer { handle: Handle },
    PeerUnreachable { handle: Handle, source: ... },
    TaskNotFound { id: TaskId },
    TaskTerminal { id: TaskId, state: TaskState, context_id: String },
    InboxFull { capacity: usize },
    EtcdUnavailable { source: ... },
    Timeout { waited: Duration },
    Internal(...),
}
```

Errors are read by a model that will otherwise retry the same call. Each states
what happened and what to do next:

> `Task task-9f2 is completed. A2A tasks cannot be modified once terminal. To follow up, call mesh_send with context_id "ctx-abc" and reference_task_ids ["task-9f2"].`

> `This instance has not announced itself, so it has no identity and no peers can see it. Call mesh_announce with a name, description, and the skills this agent can offer.`

An unreachable peer fails that call only. A peer whose etcd key has expired is
reported as gone rather than as a network error, since the node can tell the
difference (§5.6).

---

## 10. Observability

`tracing` spans keyed by `contextId` and `taskId`. Logs go to stderr — stdout
carries MCP stdio traffic and writing to it corrupts the protocol stream.
Message content is logged at `TRACE` only.

---

## 11. Testing

| Layer | Approach |
|---|---|
| Task state machine | Unit tests: terminal states absorbing, no illegal transitions |
| `mesh_send` outcomes | All three shapes: peer returns a Message; peer returns a Task terminating inside the window; peer returns a Task still running at timeout |
| Content mapping | Round-trip property tests preserving bytes, `mediaType`, `filename` |
| A2A conformance | Drive the node with `a2acli` from the `a2a-rs` workspace; talk to its `helloworld` example |
| Discovery | `etcdrs-test` provides `rstest` fixtures that spawn real etcd instances from a vendored binary |
| Announcement gating | Assert peer-facing tools return `NotAnnounced`, the card endpoint 404s, and no etcd key exists before the first announce |
| Peer death | Register two nodes, kill one, assert the survivor sees the key expire within the TTL |
| Two-node | Two instances, one etcd: A discovers B, sends, B replies, A receives |
| MCP surface | MCP Inspector; scripted client for the task extension |

---

## Appendix — verified facts

| Fact | Source |
|---|---|
| A2A v1.0.0, Linux Foundation | a2a-protocol.org/latest/specification |
| Agent card at `/.well-known/agent-card.json` | A2A agent discovery |
| Part is `oneof` text/raw/url/data + mediaType/filename/metadata | A2A core concepts |
| Terminal tasks immutable; follow up with new task in same contextId | A2A life of a task |
| Agent may respond with either a Message or a Task | A2A life of a task |
| `rmcp` 3.1.0, official Rust MCP SDK, Apache-2.0 | docs.rs/rmcp |
| `rmcp::task_manager` implements SEP-2663 (get/cancel/update, TTL, input_required) | docs.rs/rmcp |
| `a2a-lf` 0.3, `a2a-client-lf` 0.2.1, `a2a-server-lf` 0.4.1, Apache-2.0 | docs.rs |
| `a2a-server-lf` exposes `AgentCardProducer` trait, `TaskStore` trait, `InMemoryTaskStore` | docs.rs/a2a-server-lf |
| `a2a-server-lf` on axum 0.8; `a2a-client-lf` on reqwest 0.13 | docs.rs dependency lists |
| `etcdrs` is a tonic/tokio etcd v3 client, Apache-2.0 | github.com/tgockel/etcdrs |
| `ClientBuilder::connection_string` parses a comma-separated list; one endpoint connects lazily, several are balanced | etcdrs/src/client/builder.rs |
| `ClientBuilder` also offers `configure_endpoint`, `credentials`, `auth_token`, retry policy | etcdrs/src/client/builder.rs |
| `etcdrs-util` provides `cache::CacheClient` and `lease_pool::LeasePool`, both default features | etcdrs-util/Cargo.toml and README |
| `CacheClient` serves configured ranges from a watch-maintained local store, passes through when freshness is unprovable, never blocks on construction | etcdrs-util/src/cache/mod.rs |
| `LeasePool` grants leases, groups by TTL, keeps alive in a background task | etcdrs-util README |
| `etcdrs-test` provides rstest fixtures backed by a vendored etcd binary | etcdrs README |
