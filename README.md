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
- [x] **Milestone 8: Full-System Browser Verification and Product UX Audit** (Playwright E2E audit, live SSE proof, approval flows, restart recovery, zero mocked state)
- [x] **Milestone 9: Developer-Grade Productization and Daily-Use Readiness** (CLI `init`/`status`/`serve`, workspace management, git diff viewer, terminal streaming, secret redaction, unified review surface, provider matrix, retention pruning, crash recovery)

---

## CLI Subcommands

The `plexis` binary exposes a developer-facing CLI for day-to-day workspace and server operations:

```bash
# Initialize a new workspace in a git repository
plexis init /path/to/project --name my-project

# Show system status — workspaces, VCS branches, workflow/task counts
plexis status

# Start Plexis API server and SPA on the specified port
plexis serve --port 3000

# All commands accept --db to use a non-default database path
plexis status --db /tmp/custom.db
plexis serve --db /tmp/custom.db --port 4010
```

Environment variables:
```bash
# Enable hardened authentication (all API calls require Bearer token)
PLEXIS_AUTH_TOKEN=my-secret-token plexis serve

# Client-side: authenticate via Authorization header or query param
curl -H "Authorization: Bearer my-secret-token" http://localhost:3000/api/v1/workspaces
# OR
curl "http://localhost:3000/api/v1/workspaces?token=my-secret-token"
```

---

## Developer Daily-Use Workflow

```text
developer
    │
    ▼  plexis init /project --name my-app
registers workspace in authoritative DB
    │
    ▼  opens http://localhost:3000
authenticated SPA dashboard loads
    │
    ▼  selects workspace via Workspace Switcher
active workspace bound to header badge
    │
    ▼  creates workflow with objective
Plexis autonomously plans DAG and schedules agents
    │
    ▼  observes live task progression in interactive DAG
SSE events update task state in real time
    │
    ▼  reviews "Workspace Diff" tab
git diff rendered inline — added/removed lines syntax highlighted
    │
    ▼  commits clean changes via UI button
single-click commit scoped to workspace
    │
    ▼  clicks "Review Surface" on any task
unified 4-tab modal: Code Diff, Terminal Output, Verification, 8 Diagnostics
    │
    ▼  approves sensitive action gate
human sign-off recorded, task unblocked
    │
    ▼  opens Usage tab
provider capability matrix with real-time pricing per 1M tokens
    │
    ▼  triggers retention pruning
old records pruned with audit report
    │
    ▼  crash recovery (SIGTERM + restart)
database state fully reconciled — zero data loss
```

---

## Additional API Endpoints (Milestone 9 Additions)

- `GET /api/v1/workspaces` — List registered project workspaces
- `POST /api/v1/workspaces` — Register a new workspace
- `GET /api/v1/workspaces/:id/git/status` — Live git branch/dirty/head state
- `GET /api/v1/workspaces/:id/git/diff` — Full workspace diff (unified format)
- `POST /api/v1/workspaces/:id/git/commit` — Commit current working-tree changes
- `GET /api/v1/tasks/:id/terminal` — Live terminal output for a task (redacted)
- `POST /api/v1/tasks/:id/terminal` — Inject terminal lines (runtime use)
- `GET /api/v1/providers/capabilities` — Provider + model capability matrix with pricing
- `POST /api/v1/retention/prune` — Prune historical records by age
- `GET /api/v1/github/repos` — List GitHub repositories
- `GET /api/v1/github/pulls` / `POST` — List or create pull requests
- `GET /api/v1/github/issues` — List issues

---

## E2E Browser Verification

Run the comprehensive Playwright audit against a live Plexis server:

```bash
# Install Playwright once
cd web && npx playwright install chromium

# Run the Milestone 9 developer-grade E2E audit
node web/tests/e2e_milestone9.mjs
```

The test verifies all 8 product scenarios with authoritative browser proof:
1. CLI `init` and `status` subcommands
2. Hardened auth middleware (Bearer + query param)
3. Workspace switcher and registration modal
4. Provider capability matrix and token pricing
5. Historical data retention pruning
6. Workflow creation, DAG rendering, git diff, and commit
7. Terminal streaming with AWS key secret redaction
8. Unified review surface (8 diagnostics + operator approval + crash recovery)
