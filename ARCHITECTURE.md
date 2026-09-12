# Plexis Architecture

This document describes the architectural foundation and subsystem boundaries of **Plexis**.

For the exhaustive specification, see [`Plexis Architecture Specification v0.1.md`](./Plexis%20Architecture%20Specification%20v0.1.md).

---

## 1. High-Level Architecture

Plexis strictly separates the **Control Plane** (orchestration, planning, scheduling, policies) from the **Execution Plane** (agent sessions, tool sandboxes, provider adapters), united by **Durable State**:

```text
                         PLEXIS
                            │
                    ┌───────┴────────┐
                    │                 │
              Control Plane     Execution Plane
                    │                 │
          ┌─────────┼─────────┐   ┌───┼─────────────┐
          │         │         │   │   │             │
       Planner   Scheduler  Policy Agent Runtime   Tools
          │         │         │   │   │             │
          └─────────┼─────────┘   │   ├─ Providers
                    │             │   ├─ Sessions
               Task Graph         │   ├─ Sandboxes
                    │             │   └─ Messages
                    └──────┬──────┘
                           │
                    Durable State
                           │
                ┌──────────┼──────────┐
                │          │          │
              State       Events    Memory
                │          │          │
                └──────────┼──────────┘
                           │
                       API Layer
                           │
                       Web UI
```

---

## 2. Core Subsystems and Boundaries

### 2.1 Domain Layer (`plexis-core`)
- **Zero dependencies** on databases, network stacks, or specific AI providers.
- **Strongly Typed Identifiers**: UUIDv7-backed newtypes (`TaskId`, `WorkflowId`, `AgentId`, `LeaseId`, `CommandId`, etc.) with type-safe formatting (`task_...`, `wf_...`).
- **State Machines**: Explicit validated state transitions (`TaskState`, `AgentState`, `WorkflowState`, `ExecutionState`, `CommandState`).
  - *Invariant*: Task state and agent execution state are distinct. An agent can be `WaitingForInput` while the task is `Running`.
- **Task Graph (DAG)**:
  - Directed acyclic graph with deterministic cycle detection (Kahn's algorithm).
  - Topological sorting and runnable task resolution.
  - Dynamic task decomposition with automatic dependency rewiring.
  - *Invariant*: Ephemeral view 100% reconstructible from durable state.

### 2.2 Persistence Layer (`plexis-storage`)
- **Repository Pattern**: Domain logic interacts strictly via asynchronous traits (`TaskStore`, `WorkflowStore`, `AgentStore`, `CommandStore`, `LeaseStore`, `EventStore`).
- **SQLite Backend**: Transactional single-machine local store with WAL mode, foreign key enforcement, and embedded versioned SQL migrations.
- **Idempotency**: Unique constraint on `commands.idempotency_key` ensures duplicate dispatches are rejected deterministically.

### 2.3 Runtime & Concurrency (`plexis-runtime`)
- **Fencing Token Leases**: Monotonically incrementing generation counters (`generation: u64`) prevent stale or partitioned workers from committing writes to reclaimed tasks.
- **Command Queue & Dispatcher**: Bridges asynchronous requests from the scheduler to execution workers.
- **Reconciliation Engine**: Detects divergence between durable expected state (e.g. active leases, assigned tasks) and runtime reality (crashed processes, expired deadlines), conservatively returning tasks to `Ready` without data loss.

### 2.4 API-First Server (`plexis-server`)
- Axum-based HTTP server exposing REST endpoints for workflows, tasks, agents, commands, and audit events.
- Central gateway for future Web UI and CLI tooling.

---

## 3. Dependency Direction

```text
┌─────────────────┐
│  plexis-server  │
└────────┬────────┘
         │
┌────────▼────────┐
│  plexis-runtime │
└────────┬────────┘
         │
┌────────▼────────┐
│ plexis-storage  │
└────────┬────────┘
         │
┌────────▼────────┐
│   plexis-core   │
└─────────────────┘
```

Core domain logic never imports runtime, storage, or server code.
Storage implementations depend on `plexis-core`.
Runtime depends on `plexis-core` and `plexis-storage`.
Server coordinates runtime and storage.
