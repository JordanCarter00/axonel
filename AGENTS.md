# Plexis Contributor & Agent Guidelines

This document defines the rules, invariants, and conventions for engineers and AI agents implementing or extending **Plexis**.

---

## 1. Source of Truth Hierarchy

When making architectural or implementation decisions, adhere strictly to this hierarchy:

```text
1. Plexis Architecture Specification & Invariants (ARCHITECTURE.md)
2. Domain Invariants and Type Safety (plexis-core)
3. Storage & Concurrency Guarantees (plexis-storage, plexis-runtime)
4. Existing Passing Test Suites and Verification Contracts
5. Platform / Compiler Constraints
```

---

## 2. Inviolable Architectural Invariants

1. **Durable State is Authoritative**:
   - Live processes, LLM worker threads, and network connections are ephemeral.
   - Any state needed to resume, recover, or audit work must be stored durably in the database.
   - The in-memory `TaskGraph` must always be 100% reconstructible from durable storage.

2. **Planning is Decoupled from Execution**:
   - The planner (which may consult an LLM) produces execution plans and graph mutation proposals.
   - The planner must **never** directly own subprocesses, shell execution, tools, or direct database mutations.
   - All proposals pass through `PlanValidator` with strict `PlannerBudgets` before entering the task graph.

3. **Deterministic Invariants Stay Deterministic**:
   - **Never** use an LLM for dependency validation, cycle detection, state-transition legality, lease token checks, budget enforcement, or authorization.
   - LLMs handle semantic reasoning, code generation, and plan suggestions.

4. **Agents are Replaceable**:
   - Tasks belong to Plexis, not to the executing agent.
   - Task assignment uses leases protected by monotonic generation fencing tokens (`lease_generation`).
   - If an agent crashes or stalls, the reconciler reclaims the task and reassigns it with failure evidence.

5. **Everything Important is Idempotent**:
   - Commands must specify unique `idempotency_key`s enforced by unique database indexes.
   - Duplicate submissions must result in safe deduplication without double-execution.

6. **Independent Verification is Mandatory**:
   - A task cannot reach `TaskState::Verified` simply because an agent claimed completion.
   - Verification must be run by an independent verifier and recorded as durable evidence.

7. **Authority Boundaries & Privilege Separation**:
   - Agents are explicitly forbidden from mutating `System` scope memories via tools.
   - File system access must be restricted to configured workspace roots.
   - Symlink traversal attempts resolving outside the workspace root must be rejected immediately.

8. **Secret Protection & Data Hygiene**:
   - Sensitive credentials, API keys (`sk-`, `ghp_`, `AKIA`, `Bearer`), and private keys must be sanitized by `SecretRedactor` before being logged, persisted, or returned to LLMs.

---

## 3. Crate and Dependency Boundaries

Plexis strictly prohibits circular dependencies and reverse-layer dependencies:

- **`plexis-core`**: Pure domain logic only. Zero database drivers, zero network code, zero async runtime dependencies.
- **`plexis-storage`**: Implements repository traits (`TaskStore`, `WorkflowStore`, etc.) using SQLite. SQL queries and migrations belong here.
- **`plexis-providers`**: LLM provider traits and adapters (OpenAI, Anthropic, Gemini, Ollama) and failover logic.
- **`plexis-tools`**: Tool definitions, execution backends (`BubblewrapBackend`, `HostProcessBackend`), and sandbox containment.
- **`plexis-memory`**: Long-term memory management and vector/keyword search engines.
- **`plexis-planner`**: Autonomous decomposition, prompt templates, and plan validation budgets.
- **`plexis-runtime`**: Scheduler, lease manager, reconciler, runner, and recovery controller.
- **`plexis-server`**: Axum HTTP server and REST control plane.

---

## 4. Coding Standards

- **Strict Clippy Compliance**: Zero warnings tolerated. All PRs and commits must pass:
  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  ```
- **Code Formatting**: All code must strictly conform to Rustfmt:
  ```bash
  cargo fmt --check
  ```
- **Explicit Strongly Typed Errors**: Use `thiserror` for library error definitions (`DomainError`, `StorageError`, `ToolError`, `ProviderError`, `PlannerError`, `RuntimeError`). Avoid untyped string errors (`anyhow` is permitted only in test binaries or CLI mains).
- **Process Cleanup**: Any spawned subprocess must configure `kill_on_drop(true)` to prevent zombie processes upon task cancellation or panic.
- **Async Concurrency**: Use Tokio cancellation tokens and structured concurrency. Never detach long-running futures without a lifecycle owner or cancellation mechanism.

---

## 5. Verification & Testing Workflow

Before committing any change:
```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Commit often with clear, descriptive messages following conventional commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`).

---

## 6. Specialized Agent Archetypes & Capability Matching

Plexis runtime enforces capability-authoritative task dispatch (`AgentSelector`). Agents register specific capabilities and domain affinities:

| Role | Domain Affinities | Standard Capability Set |
|---|---|---|
| **Planner** | `planning`, `architecture` | `decompose_task`, `validate_dependencies`, `estimate_complexity` |
| **Researcher** | `research`, `discovery` | `inspect_codebase`, `read_multiple_files`, `search_patterns` |
| **Developer** | `coding`, `implementation` | `edit_code`, `apply_patch`, `refactor_module`, `write_file` |
| **Tester** | `testing`, `qa` | `generate_unit_tests`, `run_test_suite`, `verify_coverage` |
| **Reviewer** | `review`, `security` | `audit_security`, `inspect_diff`, `check_style_guidelines` |
| **Integrator** | `vcs`, `integration` | `merge_branches`, `resolve_conflicts`, `git_checkout`, `commit` |
| **Verifier** | `verification`, `governance` | `independent_verification`, `validate_evidence`, `assert_invariants` |

### Invariants for Agent Execution:
1. **Never Bypass Agent Role Capabilities**: A task tagged with `testing` requirements must never be dispatched to an agent lacking `run_test_suite`.
2. **Terminal Stream Boundedness**: All subprocess stdout/stderr streams must flow through bounded ring buffers (max 5,000 lines) with pattern-based `SecretRedactor`.
3. **Atomic Patch Safety**: When applying diffs or writing files, set `overwrite: false` unless explicit replacement is validated by prior file inspection.
4. **Hermetic Test Execution**: Provider smoke harnesses and GitHub API clients must operate hermetically when API credentials are absent in local or CI environments.
5. **Authoritative Git Origin**: Upstream remote is `git@github.com:axonel/axonel.git` (`https://github.com/axonel/axonel`).

