# Plexis

**Plexis** is an autonomous agent and workflow operating system engineered for serious, verifiable software engineering and general-purpose autonomous workflows.

Instead of treating AI agents as ephemeral chat completions or script runners, Plexis operates as a **resilient agent operating system**:

```text
objective
   │
   ▼
planner ──(PlannerBudgets)──► deterministic validation
   │
   ▼
task graph (DAG) ───────────► topological scheduler
   │
   ▼
fencing lease manager ──────► monotonic token validation
   │
   ▼
agent execution loop ───────► sandboxed tools & secret redactor
   │
   ▼
failover provider router ───► model inference
   │
   ▼
independent verifier ───────► durable proof & evidence
   │
   ▼
recovery controller ────────► strategy mutation & loop defense
   │
   ▼
verified state & memory
```

---

## Architectural Principles

1. **Durable State is Authoritative**: Live processes and LLM sessions are temporary. All task, workflow, agent, memory, and lease states are persisted in transactional storage and survive crashes, restarts, and agent replacement.
2. **Planning is Decoupled from Execution**: The planner determines *what* needs to happen; the runtime plane determines *how* tasks are scheduled, assigned, executed in sandboxes, and verified.
3. **Deterministic Invariants Stay Deterministic**: LLMs are never used for dependency validation, state-transition legality, lease ownership, or authorization.
4. **Agents are Replaceable**: Tasks belong to Plexis. If an agent crashes or stalls, the task survives and is reclaimed via lease expiration and re-scheduled with full failure evidence.
5. **Everything Important is Idempotent**: Commands, assignments, messages, and state mutations carry explicit idempotency keys and monotonic lease fencing tokens (`lease_generation`).
6. **Independent Verification**: Agent self-reported success is never accepted as completion. Work must produce durable evidence validated by independent verifiers.
7. **Defense in Depth**: Subprocess execution is sandboxed (Bubblewrap namespace isolation or controlled host execution with `kill_on_drop`), symlink traversals outside workspace boundaries are blocked, and sensitive credentials/tokens are automatically redacted from all outputs and logs.

---

## Workspace Structure

The Plexis engine is organized into 8 focused Rust crates:

```text
plexis/
├── crates/
│   ├── plexis-core/         # Domain models, typed IDs, 10 state machines, TaskGraph DAG engine
│   ├── plexis-storage/      # Transactional SQLite backend, WAL mode, foreign keys, migrations
│   ├── plexis-providers/    # LLM provider abstraction, streaming, embeddings, failover router
│   ├── plexis-tools/        # Tool registry, Bubblewrap/Host sandboxes, secret redactor, symlink defense
│   ├── plexis-memory/       # 8-scope hierarchical memory, hybrid vector + BM25 retrieval
│   ├── plexis-planner/      # Autonomous decomposition, planner budgets, deterministic validator
│   ├── plexis-runtime/      # Scheduler, lease manager, reconciler, runner, recovery controller
│   └── plexis-server/       # Axum REST API server, approval gates, command queue, audit events
├── migrations/              # Authoritative SQL schema migrations
├── tests/                   # End-to-end integration and real-world workload tests
├── ARCHITECTURE.md          # Comprehensive architectural specification & subsystem guide
└── AGENTS.md                # Engineering conventions and agent guidelines
```

---

## Crate Summary

| Crate | Responsibility | Key Components |
|---|---|---|
| `plexis-core` | Pure domain foundation & state machines | UUIDv7 IDs, `TaskGraph` (Kahn's algo), 10 state machines (`TaskState`, `WorkflowState`, `AgentState`, `ApprovalState`, `SessionState`, etc.) |
| `plexis-storage` | Durable storage & transaction boundaries | `SqliteStore` with WAL mode, foreign keys, monotonic lease generation, command queue with idempotency |
| `plexis-providers` | Model provider integration & resilience | `LlmProvider`, `FailoverRouter` (with `FailoverDecision` tracking), OpenAI, Anthropic, Gemini, Ollama adapters |
| `plexis-tools` | Sandboxed execution & containment | `HostProcessBackend`, `BubblewrapBackend` (namespaces), symlink traversal defense, pattern-based `SecretRedactor`, `kill_on_drop` |
| `plexis-memory` | Long-term memory & context assembly | 8 scopes (`System`, `User`, `Project`, `Workflow`, `Task`, `Agent`, `Session`, `Artifact`), hybrid semantic vector + BM25 keyword search |
| `plexis-planner` | Autonomous workflow planning | `Planner`, `PlanValidator`, `PlannerBudgets` (max tasks, max workflow tasks, max decomposition depth) |
| `plexis-runtime` | Core execution and orchestration | `Scheduler`, `LeaseManager` (fencing tokens), `Reconciler` (crash recovery), `AgentRunner`, `RecoveryController` |
| `plexis-server` | Control plane & HTTP API | Axum REST endpoints, structured error handling (`ApiError`), request body limits, approval endpoints (`/api/v1/approvals`) |

---

## Getting Started

### Prerequisites

- Rust 1.80+ (Rust 2021 edition)
- Cargo
- `bubblewrap` / `bwrap` (optional, for Linux namespace sandboxing; falls back gracefully to host execution)

### Building and Testing

Check the entire workspace:
```bash
cargo check --workspace --all-targets
```

Run linter with zero warnings allowed:
```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Format verification:
```bash
cargo fmt --check
```

Run the complete test suite:
```bash
cargo test --workspace
```

### Running the API Server & Operational Dashboard

Start the Plexis control plane server (which serves both the REST API and the React SPA dashboard):
```bash
cargo run -p plexis-server
```

Or run the frontend development server:
```bash
cd web
npm install
npm run dev
```

Build the production frontend assets:
```bash
cd web
npm run build
```

Health check:
```bash
curl http://127.0.0.1:3000/health
# {"status":"ok","version":"0.1.0"}
```

Control Plane Endpoints & SSE Streaming:
- `GET /` — Operational SPA Dashboard (Vite + React + Tailwind)
- `GET /health` — Control plane health and version status
- `GET /api/v1/dashboard/summary` — High-level operational metrics (KPIs, active workflows, leases, provider health)
- `POST /api/v1/workflows` — Create autonomous workflows with optional `auto_plan` and `auto_start`
- `GET /api/v1/workflows` — List workflows
- `GET /api/v1/workflows/:id` — Inspect workflow details and state
- `GET /api/v1/workflows/:id/graph` — Authoritative DAG nodes and dependency edges
- `POST /api/v1/workflows/:id/plan` — Trigger autonomous DAG planning
- `POST /api/v1/workflows/:id/start` — Dispatch workflow execution
- `POST /api/v1/workflows/:id/pause` / `resume` / `cancel` — Runtime lifecycle controls
- `GET /api/v1/tasks/:id/dependencies` — Task prerequisites, dependents, and diagnostic state explanations
- `POST /api/v1/tasks/:id/reassign` — Reassign task to a different specialized agent
- `GET /api/v1/approvals` — List human-in-the-loop approval requests (filterable by status)
- `POST /api/v1/approvals/:id/approve` — Approve pending execution gate with operator notes
- `POST /api/v1/approvals/:id/reject` — Reject pending execution gate with rationale
- `GET /api/v1/events/stream` — Real-time SSE event stream with `Last-Event-ID` / `?after={seq}` cursor replay
- `GET /api/v1/events/cursor` — Batch catch-up replay for missed events

---

## Milestone Roadmap

- [x] **Milestone 1: Repository Foundation & Core Architecture** (Core crates, typed IDs, state machines, SQLite persistence, leases)
- [x] **Milestone 2: Execution Plane, Providers, Sandboxing & Verification** (Tools, sandboxes, provider adapters, independent verifier)
- [x] **Milestone 3: Scheduler, Planner, Recovery Controller & Multi-Agent Engine** (DAG scheduling, dynamic decomposition, strategy mutation)
- [x] **Milestone 4: Persistent Memory, Semantic Context & Runtime Hardening** (8 memory scopes, hybrid vector + BM25, recovery controller)
- [x] **Milestone 5: Real-World Autonomous Software Workloads** (Autonomous repository modification, integration verification, crash resumption)
- [x] **Milestone 6: Production Readiness & Architecture Audit** (State transition matrix, lease generation fencing, symlink defense, secret redaction, failover tracking, planner budgets, API hardening)
- [x] **Milestone 7: Operational Dashboard and Product Interface** (React SPA in `web/`, interactive SVG DAG task graphs, SSE reconnect cursors, human governance center, task state diagnostics, provider health matrix)

