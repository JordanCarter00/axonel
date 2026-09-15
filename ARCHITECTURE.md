# Axonel Architecture

This document describes the architectural foundation, subsystem boundaries, domain invariants, and operational guarantees of **Plexis**.

---

## 1. Architectural Topology & Layering

Plexis is designed as an **autonomous agent operating system** for verifiable software development and resilient distributed agent workflows. It enforces a strict unidirectional dependency graph across 8 modular crates:

```text
┌─────────────────────────────────────────────────────────────┐
│                        plexis-server                        │
│              (HTTP API, Approvals, SSE Events)              │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                       plexis-runtime                        │
│   (Scheduler, Runner, Lease Manager, Reconciler, Recovery)  │
└───┬──────────────────────────┬──────────────────────────┬───┘
    │                          │                          │
┌───▼─────────────┐   ┌────────▼────────┐   ┌─────────────▼───┐
│ plexis-planner  │   │  plexis-memory  │   │  plexis-tools   │
│ (Budgets, DAG)  │   │ (Vector + BM25) │   │ (Sandbox, Bwrap)│
└───┬─────────────┘   └────────┬────────┘   └─────────────┬───┘
    │                          │                          │
    │                 ┌────────▼────────┐                 │
    │                 │ plexis-providers│                 │
    │                 │(LLMs, Failover) │                 │
    │                 └────────┬────────┘                 │
    │                          │                          │
┌───▼──────────────────────────▼──────────────────────────▼───┐
│                       plexis-storage                        │
│         (SQLite Repositories, WAL, Foreign Keys, Tx)        │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                         plexis-core                         │
│        (Domain Entities, Typed IDs, State Machines)         │
└─────────────────────────────────────────────────────────────┘
```

### Layering Rules:
- **`plexis-core`**: The foundational domain crate. Zero dependencies on databases, HTTP libraries, or AI providers. Contains typed identifiers (UUIDv7), 10 explicit domain state machines, and the deterministic `TaskGraph` DAG engine.
- **`plexis-storage`**: Implements asynchronous repository traits (`TaskStore`, `WorkflowStore`, `CommandStore`, `LeaseStore`, `EventStore`, `MemoryStore`, `ApprovalStore`, `SessionStore`, `RecoveryStore`) using SQLite in WAL mode with foreign key enforcement and embedded migrations.
- **`plexis-providers`**: Abstracts model providers (`OpenAiProvider`, `AnthropicProvider`, `GeminiProvider`, `OllamaProvider`, `MockProvider`) and implements a resilient `FailoverRouter` that tracks `FailoverDecision` records for latency, rate limits, and provider outages.
- **`plexis-tools`**: Secure execution boundary for external actions. Supports `BubblewrapBackend` (Linux namespaces) and `HostProcessBackend` (with strict timeouts and `kill_on_drop`), symlink traversal escape defense, and automated pattern-based `SecretRedactor`.
- **`plexis-memory`**: Long-term context and knowledge retention across 8 distinct scopes (`System`, `User`, `Project`, `Workflow`, `Task`, `Agent`, `Session`, `Artifact`). Implements hybrid retrieval combining cosine vector embeddings with BM25 keyword matching. Enforces authority boundaries (agents cannot write to `System` scope).
- **`plexis-planner`**: Autonomous task decomposition. Uses `PlanValidator` and `PlannerBudgets` (`max_tasks_per_plan`, `max_workflow_tasks`, `max_decomposition_depth`) to deterministically validate plans and guard against runaway self-recursive task explosions.
- **`plexis-runtime`**: The operating system engine. Orchestrates topological scheduling, monotonic fencing token leases (`lease_generation`), command dispatching with idempotency, crash reconciliation, and iterative strategy-mutating recovery.
- **`plexis-server`**: Axum-based control plane HTTP server. Exposes structured REST endpoints, human-in-the-loop approval gates (`/api/v1/approvals`), request payload limits (`DefaultBodyLimit`), and audit event streaming.

---

## 2. Domain State Machines

Plexis models all operational lifecycles through 10 explicit, validated state machines in `plexis-core`:

```text
1. WorkflowState:  Pending -> Running -> Completed / Failed / Cancelled
2. TaskState:      Pending -> Ready -> Assigned -> Running -> Completed -> Verified
                              └───────> Failed / Blocked / Cancelled
3. AgentState:     Idle -> Busy -> Stalled / Error / Terminated
4. ExecutionState: Starting -> Running -> WaitingForInput -> Succeeded / Failed / TimedOut
5. CommandState:   Pending -> Dispatched -> Delivered -> Succeeded / Retrying / DeadLettered
6. LeaseState:     Active -> Expired -> Revoked / Reclaimed
7. ApprovalState:  Pending -> Approved / Rejected / Expired / Cancelled
8. MemoryState:    Active -> Archived -> Deleted
9. RecoveryState:  Pending -> InProgress -> Succeeded / Failed / Escalated
10. SessionState:  Active -> Paused -> Closed
```

### Invariants:
- State transitions are validated before persistence via dedicated methods (e.g. `TaskState::can_transition_to`, `ApprovalRecord::approve`, `Session::close`). Illegal transitions return `DomainError::InvalidStateTransition`.
- Task state and agent execution state are distinct. An agent may be `WaitingForInput` while the overall task remains `Running`.

---

## 3. Concurrency, Distributed Leases & Fencing Tokens

To operate safely across concurrent worker threads or restarted processes without data corruption, Plexis implements monotonic generation fencing:

```text
Worker 1: Acquire Lease (Token: lease_id, generation: 1)
   │
   ├─► Stalls / Network Partition...
   │
Reconciler: Lease Expires -> Increments generation to 2 -> Reclaims Task
   │
Worker 2: Acquire Lease (Token: lease_id, generation: 2) -> Executes & Completes
   │
Worker 1: Wakes up -> Attempts write with generation: 1
   │
   ▼
LeaseStore: REJECTED (generation 1 < active generation 2)
```

1. **Monotonic Generation Fencing**:
   - Every lease increment assigns a monotonic `generation: u64`.
   - All worker commands generated by the `Scheduler` carry both `lease_id` and `lease_generation`.
   - Before executing and before committing results, `AgentRunner` validates its lease token against `LeaseStore`. If the generation does not match the active lease, execution is aborted immediately.

2. **Command Idempotency**:
   - Every command carries an authoritative `idempotency_key`.
   - The SQLite store enforces a unique index on `commands.idempotency_key`.
   - Re-dispatching an identical command returns the existing record without duplicating work or side effects.

3. **Orphan Command Reconciliation**:
   - Upon node crash or restart, `reconcile_commands` inspects in-flight commands in `Dispatched` or `Delivered` state whose lease has expired or whose worker has terminated, returning them safely to `Retrying` or `DeadLettered`.

---

## 4. Storage and Transactional Integrity

- **SQLite WAL Mode**: `PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;`.
- **Foreign Key Enforcement**: `PRAGMA foreign_keys = ON;` is strictly enforced on every connection. Hierarchies (`Workflow` -> `Task` -> `Approval` / `Lease`) maintain referential integrity.
- **Atomic DAG Decomposition**: When a task is decomposed into subtasks, subtask insertion, parent-task dependency rewiring, and parent state transition to `Decomposed` execute inside a single transactional block (`decompose_task_transactional`).
- **Immutable Event Sourcing Audit Trail**: All state transitions emit typed events to `EventStore` with monotonic sequencing (`seq_no`), timestamp, source, and payload. Events are append-only.

---

## 5. Sandboxed Tool Execution & Security Boundaries

Plexis operates external tools under strict containment:

1. **Isolation Backends**:
   - **`BubblewrapBackend`**: Utilizes Linux namespaces (`unshare(CLONE_NEWPID | CLONE_NEWNET | CLONE_NEWNS)`), read-only root bind-mounts, temporary private `/tmp`, and isolated user IDs.
   - **`HostProcessBackend`**: Controlled host process spawning for environments lacking unprivileged user namespaces.
   - **Process Lifecycle Protection**: Both backends explicitly configure `kill_on_drop(true)` on `tokio::process::Child` to guarantee that abandoned or cancelled execution tasks never leave orphaned zombie processes behind.

2. **Symlink Traversal Defense**:
   - `resolve_safe_path` verifies both the lexically normalized path and the canonicalized filesystem target (`canonicalize()`). If a symlink points outside the permitted workspace directory, the operation is blocked with a security violation error.

3. **Automated Secret Redaction**:
   - `SecretRedactor` scans all tool outputs, error streams, command arguments, and logs against high-entropy token patterns (`Bearer`, `sk-`, `ghp_`, `AKIA`, private keys) and user-registered confidential tokens.
   - Matched credentials are automatically stripped and replaced with `[REDACTED]` prior to storage, logging, or passing to LLM context.

---

## 6. Long-Term Memory & Hybrid Context Assembly

Plexis organizes memories into 8 distinct hierarchical scopes:
- **`System`**: Core operational rules and governance policies (read-only for agents).
- **`User`**: User identity, explicit preferences, and persistent configurations.
- **`Project`**: Repository architecture, coding style, tech stack constraints.
- **`Workflow`**: Plan state, decomposition records, execution decisions.
- **`Task`**: Evidence, verification logs, intermediate reasoning.
- **`Agent`**: Role-specific instructions, capability summaries.
- **`Session`**: Ephemeral interactive turn history.
- **`Artifact`**: Generated files, diffs, patches, and build artifacts.

### Retrieval Semantics:
- **Hybrid Retrieval**: Combines semantic cosine similarity vector search (`VectorStore`) with BM25 keyword matching (`MemoryStore`), weighted and normalized for maximum recall.
- **Soft-Delete Lifecycle**: Memories transition from `Active` to `Archived` or `Deleted`. Queries default to filtering out deleted records while allowing explicit historical audit queries.
- **Authority Enforcement**: Agents are forbidden from mutating `System` scope memories via the `save_memory` tool.

---

## 7. Autonomous Planning & Budget Invariants

The `plexis-planner` crate decomposes high-level user objectives into executable DAGs:
- **Deterministic Validation**: Every generated plan passes through `PlanValidator` before execution.
- **Resource Budgets (`PlannerBudgets`)**:
  - `max_tasks_per_plan`: Upper bound on tasks in a single generated decomposition (default: 50).
  - `max_workflow_tasks`: Upper bound on total tasks across an entire workflow lifecycle (default: 200).
  - `max_decomposition_depth`: Upper bound on recursive subtask decomposition depth (default: 5).
- **Cycle Prevention**: Dependency cycles are detected in polynomial time using Kahn's algorithm before tasks are admitted into durable storage.

---

## 8. Human-in-the-Loop Governance & Approvals

For high-risk operations (destructive disk modifications, network deployment, credential access):
- **Approval Gate Interception**: Tasks requiring policy checks are placed in `TaskState::Blocked` or generate an `ApprovalRecord` in `ApprovalState::Pending`.
- **API Gateways**: Decisions are submitted via `POST /api/v1/approvals/:id/approve` or `POST /api/v1/approvals/:id/reject`.
- **Durable Event Audit**: Every approval or rejection records the decider, timestamp, and optional rationale in the immutable event log.

---

## 9. Independent Verification

In Plexis, an agent is never permitted to self-certify completion:
- **Verification Engine**: When a task reaches `TaskState::Completed`, it is passed to an independent verifier configured for the task's contract (e.g. test execution, lint checks, compilation status).
- **Durable Proof**: If verification passes, the task transitions to `TaskState::Verified` with recorded proof. If verification fails, the task transitions to `TaskState::Failed` and triggers the `RecoveryController`.

---

## 10. Fault Tolerance & Crash Resumption

Plexis is designed to survive sudden process termination, kernel panic, or node failure:
1. **Startup Reconciliation**: On boot, the `Reconciler` scans the database for:
   - Expired leases -> reclaims tasks back to `TaskState::Ready`.
   - Orphaned commands in `Dispatched` or `Delivered` state -> transitions to `Retrying`.
   - Running workflows -> restores DAG state and resumes scheduling unblocked tasks.
2. **Dynamic Strategy Mutation**: The `RecoveryController` tracks retry attempts and mutates strategies (e.g. exponential backoff, agent role rotation, prompt refinement) while detecting and aborting infinite failure loops.

---

## 11. Operational Dashboard & Control Plane Interface

Milestone 7 introduces an operational web interface (`web/`) turning the Plexis control plane into a high-visibility developer cockpit:

```text
┌─────────────────────────────────────────────────────────────┐
│                    React SPA Dashboard                      │
│   (Vite + TypeScript + Tailwind CSS + Lucide Icons)        │
│                                                             │
│   ┌───────────────┐ ┌────────────────┐ ┌────────────────┐  │
│   │ Interactive   │ │ Real-Time      │ │ Human          │  │
│   │ SVG DAG Graph │ │ Live Timeline  │ │ Governance     │  │
│   └───────────────┘ └────────────────┘ └────────────────┘  │
└──────────────────────────────┬──────────────────────────────┘
                               │ HTTP REST & SSE
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                   Plexis Control Plane                      │
│                    (plexis-server)                          │
│                                                             │
│   - Static Asset Serving (web/dist fallback)                │
│   - SSE Event Streaming with Reconnect Cursors (?after=seq) │
│   - Graph Topological Layout & State Diagnostic Endpoints   │
│   - Task Reassignment & Agent Direct Messaging              │
│   - Local v1 Authentication (PLEXIS_AUTH_TOKEN)             │
└─────────────────────────────────────────────────────────────┘
```

### Key Subsystems:
1. **Interactive SVG DAG Task Graph**:
   - Computes topological layers from authoritative database dependencies (`TaskGraph`).
   - Renders interactive nodes with status badges, assigned agent roles, priority indicators, and verification proof markers.
   - Smooth cubic bezier curves illustrate prerequisite-dependent relations with interactive selection highlights.
2. **Authoritative State Diagnostics**:
   - Flyout drawer answers *"Why is this task in its current state?"* by analyzing unfulfilled prerequisites, active leases, verification evidence, and failure recovery attempts.
   - Allows operator-driven task reassignment across specialized agents.
3. **SSE Live Streaming & Reconnect Guarantees**:
   - Clients connect to `GET /api/v1/events/stream` passing `Last-Event-ID` or `?after={seq}`.
   - The server replays any missed historical events from `SqliteStore` before transitioning into live broadcast streaming, ensuring zero lost events across network reconnects.
4. **Governance Center**:
   - High-risk actions intercepted by approval gates are reviewed with complete parameter context and risk ratings.
   - Decisions (Approve/Reject) with operator notes or rationale are committed transactionally to storage and logged to the immutable event log.
5. **Static Single-Page Application Serving**:
   - Production assets built to `web/dist` are served directly by `plexis-server` via `tower_http::services::ServeDir`, providing a zero-external-dependency operational interface on port `3000`.

---

## 10. Milestone 9: Developer-Grade Daily-Use Interface

### CLI Layer (`plexis-server/src/cli.rs`)

The `plexis` binary exposes three subcommands backed by the same authoritative SQLite storage used by the REST server:

| Subcommand | Purpose |
|---|---|
| `plexis init <path> --name <name>` | Registers a git directory as a tracked Plexis workspace; detects git remote/branch/head |
| `plexis status` | Displays workspace count, VCS state, workflow/task/agent counts |
| `plexis serve --port <port>` | Starts the Axum control plane server with optional `PLEXIS_AUTH_TOKEN` |

### Workspace Management API

`GET|POST /api/v1/workspaces`, `GET /api/v1/workspaces/:id/git/status`, `GET /api/v1/workspaces/:id/git/diff`, `POST /api/v1/workspaces/:id/git/commit`

Workspaces are first-class entities with canonical path, VCS state (branch, head SHA, dirty flag). The diff viewer and commit button allow operators to inspect and commit agent-driven changes directly from the dashboard.

### Terminal Streaming

`GET|POST /api/v1/tasks/:id/terminal`

Each task execution streams stdout/stderr through an in-memory `TerminalBuffer` with automatic `SecretRedactor` pattern application. The GET endpoint returns accumulated lines (redacted); the POST endpoint accepts line injection for runtime use or testing.

### Unified Task Review Surface

The `TaskReviewModal` provides a 4-tab operator interface triggered by the "Review Surface" button in `TaskDetailDrawer`:

| Tab | Content |
|---|---|
| **Code Diff** | Inline workspace diff for the task's bound workspace |
| **Terminal Output** | Full task terminal stream via `TerminalView` |
| **Verification & Criteria** | Independent verifier evidence, verdict, and acceptance criteria |
| **8 Diagnostic Answers** | Structured answers to: objective, file changes, tools, test results, approval reason, planned action, prior attempt delta, DAG placement |

Pending approvals are surfaced with Approve/Reject controls and operator note capture. Decision is committed transactionally and the task is unblocked to `Ready` on approval.

### Provider Capability Matrix (`UsageView`)

`GET /api/v1/providers/capabilities` returns per-model capabilities:
- `context_window_tokens`, `supports_tools`, `supports_streaming`, `supports_vision`
- `reasoning_tier` (none/low/medium/high)
- `pricing.cost_per_1k_input_tokens`, `pricing.cost_per_1k_output_tokens`

Rendered as an interactive table with pricing shown per 1M tokens in USD.

### Historical Retention Pruning

`POST /api/v1/retention/prune` accepts `{ max_age_days: number }` and returns a structured prune report. The dashboard exposes this as a one-click maintenance action.

### E2E Verification Suite (`web/tests/e2e_milestone9.mjs`)

A Playwright-based end-to-end audit that verifies all 8 product scenarios against a real Plexis server with real SQLite persistence, zero mocked API responses. The suite exercises the complete developer daily-use workflow from CLI registration through crash recovery.

---

## 11. Milestone 10: Real AI Coding Workflow and Product Hardening

Milestone 10 hardens the Plexis agent operating system for real developer workloads, real LLM providers, multi-agent collaboration, and production scale.

### 11.1 Real Provider Resilience & Capability Detection (`plexis-providers`)

- **Configurable Timeouts & Transient Retries**: `OpenAiProvider`, `GeminiProvider`, and `OllamaProvider` enforce request timeouts via `reqwest::ClientBuilder::timeout`.
- **Error Classification**: Timeouts, HTTP 429 rate limits, and 5xx server errors are classified as transient with retryability hints, enabling the `FailoverRouter` and agent loop to back off gracefully before switching providers.
- **Provider Smoke Harness**: Real provider integration tests verify capability flags, streaming, and tool schemas against live endpoints when environment credentials (`OPENAI_API_KEY`, `GEMINI_API_KEY`, `ANTHROPIC_API_KEY`) are present, falling back to hermetic tests when absent.

### 11.2 Capability-Authoritative Agent Selection (`plexis-runtime::selector`)

- **Dynamic Affinity Scoring**: `AgentSelector` ranks candidate agents based on domain affinities (Planner, Researcher, Developer, Tester, Reviewer, Integrator, Verifier), capability matches, and current active lease load.
- **Strict Role Specialization**: Prevents generalist agents from executing tasks requiring explicit competencies (e.g. static analysis, git conflict resolution, or independent verification).

### 11.3 Multi-Agent Concurrent Repository Workloads & Recovery

- **Repository A (Bug Diagnosis & Modulo Repair)**: Planner decomposes bug report; Developer isolates off-by-one / negative modulo bug; runs `cargo test`; commits verified fix.
- **Repository B (Concurrent Multi-Agent Token Bucket)**: Parallel execution of Developer and Tester tasks; durable cross-agent coordination via SQLite `AgentMessageStore`; tests pass concurrently; clean commit.
- **Repository C (Failure Injection & Strategy Mutation)**: Developer task injected with invalid code; `cargo test` fails; `RecoveryController` records failure evidence, mutates strategy (retry with corrective context), and re-dispatches; passes on second attempt.

### 11.4 Tool Hardening (`plexis-tools`, `plexis-server`)

- **Filesystem Tools**:
  - `read_file`: Added line slicing (`start_line`, `end_line`) and line numbering.
  - `write_file`: Added overwrite protection (`overwrite: false` fails if target exists).
  - `read_multiple_files`: Batch inspection of multiple repository files in a single tool call.
  - `apply_patch`: Safe patch application to existing files with content verification.
- **Git Tools**:
  - `checkout`: Switch branches with optional `-b` branch creation.
  - `branch`: List and manage local git branches.
  - `conflicts`: Inspect unresolved merge conflict markers.
- **Terminal Buffer**:
  - `TerminalBuffer`: Bounded ring buffer with capacity eviction (`DEFAULT_MAX_TERMINAL_LINES = 5000`) preventing memory exhaustion during verbose builds or infinite loops.

### 11.5 Control-Plane Auth Hardening (`plexis-server::auth`)

- **Constant-Time Verification**: Prevents timing-attack vulnerabilities using `constant_time_eq` comparison on API tokens.
- **Dynamic Token Rotation**: `PLEXIS_AUTH_TOKEN` supports comma-separated active tokens, allowing zero-downtime token rollover.
- **Health Probe Exemption**: `/health` and `/api/v1/health` are unauthenticated for container orchestrator and load balancer probes; all data and execution routes strictly require valid Bearer or query parameter tokens.

### 11.6 Real GitHub REST Integration (`plexis-server::github`)

- **REST Client**: Integrates with `https://api.github.com` for repository listing, PR creation, and issue inspection using `reqwest`.
- **Deterministic Fallback**: Provides realistic offline fallback structures when `GITHUB_TOKEN` is unset, ensuring hermetic testing.
- **Axonel Ownership**: Repository metadata pointed to `https://github.com/axonel/axonel`.

### 11.7 Web UI: Configuration & Accounting (`web`)

- **Provider Configuration Modal**: Users can configure active LLM providers (OpenAI, Anthropic, Gemini, Ollama, Mock) and securely store API keys in local storage.
- **Token Usage & Cost Attribution**: Displays prompt and completion token counts, estimated dollar costs, budget alert thresholds with visual alert banners, and per-agent role attribution tables.

### 11.8 Scale & Stress Testing (`tests/large_workflow_stress_tests.rs`)

- **25-Task DAG Fan-Out/Fan-In**: 5 tiers of 5 parallel tasks executed across 5 specialized agents.
- **Tick Latency Metrics**: Average scheduler tick latency monitored (~6.17 ms); database query latency tracked (~0.32 ms).
- **Database Backup & Restore**: Primary SQLite database snapshotted, state mutated, and verified restorable to pre-backup state.

### 11.9 Browser Regression Suite (`web/tests/e2e_milestone10.mjs`)

Comprehensive Playwright test covering CLI initialization, constant-time auth rejection, unauthenticated health probe, UI project selection, provider config modal, cost/token accounting, live DAG rendering, diff viewer commit, streaming terminal with secret redaction, and 8-question diagnostic task approval.



