# Plexis Contributor & Agent Guidelines

This document defines the rules, invariants, and conventions for engineers and AI agents implementing Plexis.

---

## 1. Source of Truth Hierarchy

When making architectural or implementation decisions, adhere strictly to this hierarchy:

```text
1. Plexis Architecture Specification (Plexis Architecture Specification v0.1.md)
2. Plexis ARCHITECTURE.md and domain invariants
3. Existing Plexis tests and implementation
4. Platform / compiler constraints
```

---

## 2. Inviolable Architectural Invariants

1. **Durable state is authoritative**:
   - Live processes, LLM sessions, and memory states are ephemeral.
   - Any state needed to resume, recover, or audit work must be stored durably in the database.
   - The in-memory `TaskGraph` must always be rebuildable from storage.

2. **Planning is separate from execution**:
   - The planner (which may consult an LLM) produces execution plans and graph mutation proposals.
   - The planner must **never** directly own subprocesses, shell execution, tools, or direct database mutations.
   - All proposals pass through validation and the command queue.

3. **Deterministic invariants stay deterministic**:
   - **Never** use an LLM for dependency checks, cycle detection, state-transition validation, lease token checks, or permission enforcement.
   - LLMs handle semantic decomposition, planning, and tool reasoning.

4. **Agents are replaceable**:
   - Tasks belong to Plexis, not to the executing agent.
   - Task assignment uses leases with monotonic generation fencing tokens.
   - An expired lease allows the reconciler to safely reclaim and reassign the task.

5. **Everything important is idempotent**:
   - Commands must specify unique idempotency keys.
   - Duplicate submissions must result in `StorageError::IdempotencyConflict` or safe deduplication.

6. **Independent verification**:
   - A task cannot reach `TaskState::Verified` simply because an agent claimed completion.
   - Verification must be run by an independent verifier and recorded as durable evidence.

---

## 3. Crate and Dependency Boundaries

- `plexis-core`: Pure domain logic only. Zero database drivers, zero network code.
- `plexis-storage`: Implements repository traits (`TaskStore`, `WorkflowStore`, etc.) using SQLite. SQL queries belong here.
- `plexis-runtime`: Execution plane mechanisms (command dispatch, lease management, startup and continuous reconciliation).
- `plexis-server`: API-first HTTP server (`axum`). Exposes endpoints and orchestrates storage and runtime.

---

## 4. Git Workflow

- **Small, coherent commits**: Commit every completed logical change with clear descriptions.
- **Genuine progression**: Keep Git history honest and descriptive.
- **Always verify before committing**:
  ```bash
  cargo check --workspace --all-targets
  cargo test --workspace
  ```
- **Push regularly**: Keep `origin/main` updated with completed, passing milestones.

---

## 5. Coding Standards

- Prefer explicit types over dynamic representations.
- Enforce error handling using `thiserror` (typed errors for domain and storage; avoid untyped string errors).
- Document public structs, enums, and functions with Rustdoc comments.
- Treat warnings as errors (`cargo clippy --workspace --all-targets -- -D warnings`).
