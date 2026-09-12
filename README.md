# Plexis

**Plexis** is an autonomous agent and workflow operating system engineered for serious, verifiable software engineering and general-purpose autonomous workflows.

Instead of treating AI agents as ephemeral chat completions or script runners, Plexis operates as an **agent operating system**:

```text
plan → decompose → schedule → assign → execute → coordinate → verify → recover → continue
```

---

## Architectural Principles

1. **Durable state is authoritative**: Live processes and LLM sessions are temporary. State survives crashes, machine restarts, and agent replacement.
2. **Planning is decoupled from execution**: The planner determines *what* needs to happen; the execution plane determines *how* it runs in sandboxes and tools.
3. **Deterministic invariants stay deterministic**: LLMs are never used for dependency validation, state-transition legality, lease ownership, or authorization.
4. **Agents are replaceable**: Tasks belong to Plexis. If an agent crashes or stalls, the task survives and can be reassigned with failure evidence.
5. **Everything important is idempotent**: Commands, assignments, messages, and state mutations carry explicit idempotency keys and fencing tokens.
6. **Independent verification**: Agent self-reported success is not accepted as completion. Work must produce durable evidence validated by independent verifiers.

---

## Workspace Structure

The codebase is organized as a modular Rust workspace:

```text
plexis/
├── crates/
│   ├── plexis-core/         # Domain models, typed IDs, state machines, TaskGraph DAG
│   ├── plexis-storage/      # Repositories, transactions, SQLite backend, migrations
│   ├── plexis-runtime/      # Command dispatcher, lease manager, reconciliation
│   └── plexis-server/       # API-first HTTP server (Axum) and operational endpoints
├── migrations/              # Authoritative SQL schema migrations
├── tests/                   # End-to-end integration tests
├── Cargo.toml               # Workspace definition and dependency graph
├── ARCHITECTURE.md          # Architectural guide and subsystem specification
└── AGENTS.md                # Engineering conventions and agent guidelines
```

---

## Getting Started

### Prerequisites

- Rust 1.80+ (Rust 2021 edition)
- Cargo

### Building and Testing

Check the entire workspace:
```bash
cargo check --workspace --all-targets
```

Run the complete test suite:
```bash
cargo test --workspace
```

Run the API server:
```bash
cargo run -p plexis-server
```

Health check:
```bash
curl http://127.0.0.1:3000/health
# {"status":"ok","version":"0.1.0"}
```

---

## Status and Roadmap

- [x] Repository foundation and Rust workspace
- [x] Strongly-typed domain identifiers (`TaskId`, `AgentId`, `WorkflowId`, `LeaseId`, etc.)
- [x] Explicit state models (`TaskState`, `AgentState`, `CommandState`, etc.)
- [x] First-class `TaskGraph` DAG engine with cycle detection and runnable task queries
- [x] Authoritative SQLite persistence with versioned migrations
- [x] Concurrency protection via monotonic fencing token leases
- [x] Command queue with idempotency keys
- [x] Immutable audit event trail
- [x] Runtime boundaries and reconciliation engine
- [x] API-first server skeleton
- [ ] Provider adapters (OpenAI, Gemini, Ollama)
- [ ] Sandboxed tool runtime (filesystem, shell, git)
- [ ] Deterministic scheduler & LLM planner
- [ ] Independent verification suite
- [ ] Operational Web UI
