# Implementation Plan

## Purpose and authority

This is the durable navigation guide for the `a2a-mesh` MVP. It groups the work, records accepted scope, and gives a practical execution order without repeating issue bodies.

[`DESIGN.md`](DESIGN.md) is authoritative for product behavior, protocol contracts, security rules, and architecture. GitHub issues are the implementation work. If an issue or this plan conflicts with `DESIGN.md`, reconcile it to the design before merging code.

Repository planning data:

- [`github-issue-manifest.json`](github-issue-manifest.json) maps plan IDs to created issues and records desired type, Priority, and Effort.
- [`github-issue-relationships.json`](github-issue-relationships.json) records pending direct dependency edges and the REST requests to create them.

Custom fields and native GitHub relationships are not assumed to be applied. Verify GitHub before treating either JSON file as current repository state.

## Accepted MVP decisions

- **Async MCP contract:** `mesh_send` and related operations return ordinary `message`, `completed`, or `pending` results. Mesh task tools manage pending work. Native MCP Tasks are not required.
- **Network and binding:** Secure multi-host operation is in scope. A2A advertises and accepts JSON-RPC only.
- **A2A TLS identity:** Each process creates an in-memory ECDSA P-256 self-signed certificate with its advertised DNS name or IP in the Subject Alternative Name. The lowercase-hex SHA-256 SubjectPublicKeyInfo fingerprint is stored in the lease-bound registration and exposed through mesh identity and peer results.
- **TLS defaults:** Loopback binds default to HTTP and may opt into pinned TLS. Non-loopback and wildcard binds require pinned TLS, with no plaintext override.
- **Client identity:** Process-generated outbound client certificates are configurable and enabled by default. Servers request but do not require one. A matching registered fingerprint gives a verified mesh identity; absent or unknown credentials give an unverified non-mesh identity. Non-mesh clients need the server certificate or fingerprint out of band.
- **etcd security:** Secure etcd is expected. Remote plaintext needs an explicit insecure opt-in and startup warning. MVP uses the pinned etcdrs client's connection behavior rather than defining a separate stable credential contract.
- **Registration:** A versioned agent record and separate encoded handle claim share one standalone lease and are written atomically. Reconcile unknown transaction outcomes with lease-aware checks, expected instance values, and registration versions.
- **Announcement:** `mesh_announce` succeeds only after the collision-safe etcd transaction commits. During an outage it returns `MeshUnavailable`, leaves the process unannounced, and leaves the Agent Card endpoint at HTTP 404.
- **Lease recovery:** Reclaim only the exact former handle when it remains free. If ownership changed or is inconsistent, enter `IdentityLost`; do not rename or exit. Recovery requires a new explicit `mesh_announce`.
- **Discovery outages:** Return the last peer snapshot with explicit stale/degraded health, age, and revision. There is no age cutoff; sends remain allowed. Connection failures affect only a local reachability overlay.
- **Inbox:** `mesh_inbox` peeks; reads do not dequeue, acknowledge, or change state. Messages remain pending until reply, cancellation, or deadline. Opt-in `subscriptions/listen` updates support `mesh://inbox`; polling remains authoritative and updates are never unsolicited.
- **Saturation:** Reject a full inbound queue before task creation with a stable resource-exhausted A2A error. Create no task ID or history entry.
- **Task identity and state:** Public surfaces use opaque process-generated mesh task IDs; peer A2A IDs remain separate. An internal registry maps direction, peer, and A2A ID. Preserve every A2A state as a distinct lowercase mesh state. Cancellation reports the actual peer state or a specific not-cancelable error.
- **Memory bounds:** Defaults are inbound capacity 64, deadline 15 minutes, 256 tasks per peer/context, 24-hour outbound tracking, 24-hour terminal retention, 512 MiB blob storage, and a 256 KiB inline threshold. Evict oldest terminal records and blobs first, never active tasks, and reject oversized writes with `ResourceLimit`.
- **Threat model:** Participants are trusted and cooperative. Minimal protocol validation and correctness-related bounds are required; adversarial-peer hardening is not an MVP goal.
- **Planning and docs:** User-visible behavior is a Feature; foundations, documentation, and tests are Tasks; Bug is for defects. Priority and Effort are desired custom fields. Dates stay unset until scheduling. Labels describe components, not estimates. User documentation belongs under `doc/`; issue bodies stand alone; routine pull-request checks are assumed.

## Milestones

Use the direct edges in the relationship JSON for exact prerequisites.

- **M0 — Foundations** (`FOUND-001`–`FOUND-010`): authoritative design and docs, reproducible dependencies, configuration, TLS identity, stable DTOs, handle/task primitives, errors, content/blob handling, and the MCP shell.
- **M1 — Secure discovery vertical** (`DISC-001`–`DISC-006`, then `APP-001`): registration encoding, secure etcd access, transactional ownership and lease supervision, stale-aware discovery, Agent Card serving, discovery tools, and a runnable announce-to-discover slice.
- **M2 — Inbound messaging** (`IN-001`–`IN-004`): validated A2A requests, queue and retention bounds, peek-only inbox, replies and continuations, and opt-in inbox updates.
- **M3 — Outbound and task control** (`OUT-001`–`OUT-005`): certificate-pinned clients, streaming and polling tracking, the three-outcome send contract, direction-aware task access, responses, and cancellation.
- **M4 — Runtime and verification** (`APP-002`, then `TEST-001`–`TEST-003`): process wiring and bounded shutdown, real-etcd failure tests, two-node pinned-TLS lifecycles, and external A2A/MCP conformance. This is the MVP release gate.

## Issue index

Priority and Effort are desired manifest values, not a claim that GitHub currently has them applied.

| Plan ID | Issue | Linked title | Type | Desired Priority | Desired Effort | Outcome |
|---|---:|---|---|---|---|---|
| FOUND-001 | 3 | [Define the validated architecture and complete initial documentation](https://github.com/bidetly/a2a-mesh/issues/3) | Task | Urgent | High | Establish authoritative design and repository docs. |
| FOUND-002 | 5 | [Lock dependencies and establish CI](https://github.com/bidetly/a2a-mesh/issues/5) | Task | High | Medium | Make builds reproducible and baseline checks enforceable. |
| FOUND-003 | 6 | [Add typed base configuration and resource limits](https://github.com/bidetly/a2a-mesh/issues/6) | Task | High | Medium | Provide validated settings and memory bounds. |
| FOUND-004 | 9 | [Implement TLS identity and security configuration](https://github.com/bidetly/a2a-mesh/issues/9) | Task | High | High | Generate ephemeral identity and enforce security policy. |
| FOUND-005 | 10 | [Define stable tool, resource, registration, and security DTOs](https://github.com/bidetly/a2a-mesh/issues/10) | Task | High | Medium | Freeze public and registration wire shapes. |
| FOUND-006 | 13 | [Implement handle grammar and process metadata](https://github.com/bidetly/a2a-mesh/issues/13) | Task | Medium | Low | Validate handles and gather process metadata. |
| FOUND-007 | 15 | [Implement task transitions and opaque task-id registry](https://github.com/bidetly/a2a-mesh/issues/15) | Task | Medium | Medium | Centralize state changes and task lookup. |
| FOUND-008 | 17 | [Define public errors and structured tool results](https://github.com/bidetly/a2a-mesh/issues/17) | Task | Medium | Medium | Stabilize errors and dual structured/text output. |
| FOUND-009 | 19 | [Implement content conversion, artifact assembly, and bounded blob storage](https://github.com/bidetly/a2a-mesh/issues/19) | Task | Medium | High | Preserve content and bound artifact storage. |
| FOUND-010 | 20 | [Build the MCP router shell, gating, and stderr logging](https://github.com/bidetly/a2a-mesh/issues/20) | Task | Medium | Medium | Provide MCP routing, identity gates, and safe logs. |
| DISC-001 | 22 | [Encode versioned registration and handle-claim keys](https://github.com/bidetly/a2a-mesh/issues/22) | Task | High | Low | Define record and atomic claim encodings. |
| DISC-002 | 23 | [Build the secure etcd client, cache, and raw health client](https://github.com/bidetly/a2a-mesh/issues/23) | Task | High | Medium | Create discovery access, snapshots, and health probes. |
| DISC-003 | 24 | [Implement transactional registration and lease supervision](https://github.com/bidetly/a2a-mesh/issues/24) | Task | High | High | Own identity atomically and recover safely. |
| DISC-004 | 25 | [Implement discovery health, stale snapshots, and reachability overlay](https://github.com/bidetly/a2a-mesh/issues/25) | Task | High | Medium | Label stale peers and track local reachability. |
| DISC-005 | 26 | [Build the JSON-RPC Agent Card and pre-announce 404 route](https://github.com/bidetly/a2a-mesh/issues/26) | Feature | High | Medium | Serve the card only after committed announcement. |
| DISC-006 | 27 | [Add announce, identity, and peer-discovery MCP surfaces](https://github.com/bidetly/a2a-mesh/issues/27) | Feature | High | Medium | Expose identity and discovery through MCP. |
| APP-001 | 28 | [Wire a runnable secure discovery vertical](https://github.com/bidetly/a2a-mesh/issues/28) | Feature | High | Medium | Run secure announcement and discovery together. |
| IN-001 | 12 | [Implement the validated inbound request handler, executor, store, and queue](https://github.com/bidetly/a2a-mesh/issues/12) | Task | High | High | Accept valid inbound work without unsafe side effects. |
| IN-002 | 14 | [Add saturation, deadline, continuation, and retention behavior](https://github.com/bidetly/a2a-mesh/issues/14) | Task | High | Medium | Bound inbound work and resolve deadlines cleanly. |
| IN-003 | 16 | [Add inbox, reply, and mesh://inbox surfaces](https://github.com/bidetly/a2a-mesh/issues/16) | Feature | High | Medium | Provide peek-only inbox access and replies. |
| IN-004 | 18 | [Add opt-in subscriptions/listen updates for mesh://inbox](https://github.com/bidetly/a2a-mesh/issues/18) | Feature | High | Low | Notify subscribers while polling stays authoritative. |
| OUT-001 | 21 | [Resolve peers and create certificate-pinned JSON-RPC clients](https://github.com/bidetly/a2a-mesh/issues/21) | Task | High | Medium | Select and pin discovered peer endpoints. |
| OUT-002 | 29 | [Track outbound tasks across streams and polling](https://github.com/bidetly/a2a-mesh/issues/29) | Task | High | Medium | Keep pending work observable across timeouts. |
| OUT-003 | 30 | [Add mesh_send and the three-outcome attempt state machine](https://github.com/bidetly/a2a-mesh/issues/30) | Feature | High | High | Deliver message, completed, and pending outcomes. |
| OUT-004 | 31 | [Add task reads, filters, and mesh://task/{id}](https://github.com/bidetly/a2a-mesh/issues/31) | Feature | High | Medium | Query either direction through opaque IDs. |
| OUT-005 | 32 | [Add cancellation and direction-aware task responses](https://github.com/bidetly/a2a-mesh/issues/32) | Feature | High | Medium | Continue or cancel while preserving actual state. |
| APP-002 | 4 | [Complete runtime wiring and bounded graceful shutdown](https://github.com/bidetly/a2a-mesh/issues/4) | Feature | High | Medium | Assemble services and stop them within a budget. |
| TEST-001 | 7 | [Verify real-etcd collision, outage, stale-read, and lease-loss behavior](https://github.com/bidetly/a2a-mesh/issues/7) | Task | High | High | Prove discovery behavior under etcd failures. |
| TEST-002 | 8 | [Verify two-node pinned-TLS messaging and task lifecycle](https://github.com/bidetly/a2a-mesh/issues/8) | Task | High | High | Prove secure two-process messaging and tasks. |
| TEST-003 | 11 | [Verify A2A and MCP wire conformance with pinned external tools](https://github.com/bidetly/a2a-mesh/issues/11) | Task | High | High | Confirm protocol surfaces with external clients. |

## Execution guidance

1. Read the relevant `DESIGN.md` sections before implementation. If a missing detail changes public behavior, update the design first.
2. Use plan IDs in commits and coordination. Confirm pending direct edges in GitHub before relying on native relationship state.
3. Keep configuration, DTOs, task states, errors, content, and registration contracts centralized in foundation modules; features consume them rather than create local variants.
4. Run the secure discovery vertical before integrating messaging. Inbound and outbound foundations may proceed in parallel after their direct prerequisites.
5. Never publish identity or an Agent Card before etcd ownership is proven committed. Reconcile ambiguous outcomes; do not guess or allocate another handle.
6. Always carry health, age, and revision with stale peers. Do not hide old peers or delete registrations after local connection failures.
7. Keep opaque mesh task IDs separate from A2A task IDs. Task responses and cancellation must dispatch by direction and preserve actual state.
8. Test non-routine boundaries where introduced: transaction ambiguity, lease collision, stale reads, saturation, deadline races, certificate mismatch, tracking, subscriptions, and cancellation. Update durable user documentation under `doc/` with behavior changes.

## Explicit deferrals

- Native MCP Tasks and protocol-level task notifications.
- Any A2A binding other than JSON-RPC.
- Durable storage for registrations, tasks, transcripts, or blobs.
- Adversarial-peer defenses beyond minimal validation and availability bounds.
- A crate workspace split or service decomposition.
- Operator-provided or CA-signed A2A certificates.
- A new stable etcd credential contract beyond pinned etcdrs behavior and the remote-plaintext opt-in.
- Automatic rename after lease-loss collision.
- A stale-peer age cutoff or suppression of sends to stale peers.
- Inbox dequeue or acknowledgment semantics.

Moving a deferral into MVP requires a `DESIGN.md` change followed by updates to affected issues, edges, estimates, and this plan.

## Completion rule

MVP is ready when the runtime is assembled, milestone outcomes are met, real-etcd and two-node security suites pass, pinned external wire checks pass, and repository documentation describes shipped behavior without presenting deferred work as available.
