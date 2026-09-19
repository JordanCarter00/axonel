# Axonel Release Candidate (RC1) Specification & Operating Boundary

**Status:** Release Candidate Foundation (Hardened)  
**Version:** 0.1.0-rc1  
**Author:** Axonel Architecture Team  
**Date:** September 19, 2026  

---

## 1. Executive Definition

**Axonel** is a local-first, autonomous engineering supervisor daemon designed to execute and supervise medium-horizon coding missions (bug fixing, tech-debt remediation, dependency updates, and flaky test repairs) using external CLI coding agents (such as Google Gemini CLI).

Unlike raw coding agents or interactive chat assistants, Axonel manages the entire engineering mission lifecycle:
$$\text{Objective} \to \text{Autonomous Mission} \to \text{Worktree Isolation} \to \text{External Process Supervision} \to \text{Out-of-Band Compiler Verification} \to \mathbf{AwaitingAcceptance} \to \mathbf{Explicit Acceptance} \to \text{Safe Git Integration}$$

Axonel is **not** a new foundational model or an interactive chat IDE; it is the **supervision, reliability, and lifecycle control plane** that removes the "babysitting tax" from coding agents.

---

## 2. What v1 Does

1. **Autonomous Mission Management:**
   - Durable multi-cycle execution engine backed by transactional SQLite storage.
   - Explicit lifecycle state machine (`Created` $\to$ `Planning` $\to$ `Running` $\to$ `Replanning` $\to$ `Verifying` $\to$ `AwaitingAcceptance` $\to$ `Accepted` $\to$ `Integrating` $\to$ `Integrated` / `Rejected` / `NeedsHuman` / `BudgetExhausted` / `Cancelled`).
   - Restarts seamlessly across control-plane crashes or daemon shutdowns with zero state loss.

2. **External Process Supervision:**
   - Supervises real external agents (Google Gemini CLI, scripted adapters, and fake agents).
   - Enforces multi-dimensional resource budgets (maximum execution duration, maximum concurrent processes, maximum process executions, maximum recovery attempts, maximum stagnant cycles).
   - Monitors child processes, captures stdout/stderr in memory ring buffers, and terminates process trees upon timeouts or administrative cancellations.

3. **Workspace & Worktree Confinement:**
   - Executes agent tasks within isolated Git worktrees, preserving developer work on the main branch.
   - Rejects directory traversal attempts outside workspace boundaries (`..` path traversal containment).

4. **Independent Physical Verification:**
   - Rejects agent self-reports of success.
   - Evaluates physical stopping conditions directly against the filesystem using authoritative tooling (`cargo test`, `cargo check`, clean working tree checks, and custom verifier commands).
   - Verifies that new commit objects exist in the Git object store.

5. **Explicit Human Acceptance Gate & Review Package (Milestone 19):**
   - **Autonomous execution halts at `AwaitingAcceptance`:** Autonomous missions NEVER silently integrate into the target branch upon verification.
   - **Review Package (`GET /api/v1/missions/{id}/review`):** Unified deliverable diff, changed files list, test verification status, execution metrics, warnings, target HEAD freshness (`reverification_required`), and audit trail.
   - **Explicit Human Acceptance (`POST /api/v1/missions/{id}/accept`):** Unaccepted integration returns HTTP 409 Conflict. Operator can accept with or without immediate integration.
   - **Non-Destructive Rejection (`POST /api/v1/missions/{id}/reject`):** Records mandatory rejection reason in audit trail while preserving candidate commits and worktrees on disk for inspection.

6. **True Transactional Integration, Recovery & Repository Safety (Milestone 20):**
   - **Intermediate `Integrating` State:** Logs durable intent (`integration_intent`) before touching physical Git state.
   - **Intra-Process Concurrency Control:** Enforces per-workspace mutex serialization via `WorkspaceLockManager`.
   - **Git-Authoritative Crash Recovery:** Reconciler queries `git merge-base --is-ancestor` on startup. If commit exists on disk $\to$ `Integrated`; if not $\to$ aborts merge cleanly and resets to `Accepted`.
   - **Zero False Reporting:** Axonel never reports `integrated` unless Git physically contains the deliverable.
   - **Pristine Rollback on Conflict:** Detects merge conflicts immediately, executes `git merge --abort`, and resets to `Accepted` with HTTP 409 Conflict.
   - **Target Branch Freshness Protection:** Compares against `expected_target_head`; rejects stale integration attempts.
   - **Dirty Target Working Tree Protection:** Rejects dirty working trees with HTTP 409 Conflict before touching Git.

6. **Automated Secret Redaction:**
   - Automatically sanitizes Bearer tokens, OpenAI/Anthropic API keys (`sk-...`), Google API keys (`AIza...`), GitHub tokens (`ghp_...`), AWS access keys (`AKIA...`), and PEM private keys from terminal streaming buffers, logs, and event streams.

7. **Control Plane & Developer Interfaces:**
   - REST API with Bearer token authentication or local loopback mode.
   - Real-time terminal streaming buffer (`/api/v1/tasks/{id}/terminal`).
   - Unified CLI (`axonel init`, `axonel serve`, `axonel mission create`, `axonel mission status`, `axonel mission diff`).
   - Single-page web dashboard for mission inspection and approval workflows.

---

## 3. What v1 Does NOT Do

1. **No Proprietary Foundation Model:** Axonel does not generate code directly via an internal model; it supervises external CLI tools (`gemini`, `claude`, etc.).
2. **No Interactive In-Editor Pair Programming:** Axonel is designed for asynchronous background delegation, not millisecond keystroke autocompletion.
3. **No Automatic Multi-Repo Cross-Boundary Coordination:** Each mission is strictly confined to a single workspace repository.
4. **No Automated Conflict Resolution Engine:** If an integration encounters a Git merge conflict against the target branch, Axonel aborts the merge and surfaces the conflict to the developer rather than guessing how to resolve conflicting code.
5. **No Cloud SaaS Egress:** Axonel does not upload repository source code or databases to any third-party SaaS cloud; all control plane data remains on local disk.

---

## 4. Supported Operating Assumptions & Environment

- **Operating System:** Linux (x86_64 or aarch64) with POSIX process semantics (`libc`, `killpg`, `SIGTERM`/`SIGKILL`).
- **Dependencies:**
  - `git` $\ge$ 2.34 (requires worktree support).
  - `rustc` / `cargo` $\ge$ 1.80 (for Rust crate verification).
  - `sqlite3` $\ge$ 3.35.
  - External Agent CLI: Google Gemini CLI (`gemini`) installed in `$PATH` with valid authentication (`GEMINI_API_KEY` or `gcloud auth`).
- **Filesystem Assumptions:**
  - Local POSIX filesystem supporting atomic renames and file locking for SQLite WAL mode.
  - Network filesystems (NFS, SMB) are unsupported for authoritative SQLite storage due to locking latencies.

---

## 5. Security & Isolation Boundaries

| Boundary | Enforcement Mechanism | Failure Action |
|---|---|---|
| **Filesystem Confinement** | `ToolSandbox::resolve_safe_path` checks canonical paths against root workspace directory. | Rejects with `ToolError::ConfinementViolation`. |
| **Secret Sanitization** | `SecretRedactor::redact_patterns` and `redact_value_all` applied to event streams and streaming terminal buffers. | Replaces tokens with typed redaction markers (`[REDACTED_API_KEY]`, `[REDACTED_GH_TOKEN]`). |
| **Process Termination** | `ChildGuard` process group tracking with fallback escalation (`SIGTERM` $\to$ 250ms delay $\to$ `SIGKILL`). | Reaps zombie and orphan child processes. |
| **Target Tree Hygiene** | `git status --porcelain` check prior to branch checkout and merge. | Aborts integration immediately with HTTP 409 Conflict. |
| **Merge Safety** | Non-fast-forward merge attempted with immediate `git merge --abort` on conflict. | Restores working tree to pre-merge state and returns HTTP 409 Conflict. |
| **API Authentication** | Constant-time bearer token comparison on all `/api/v1/*` routes. | Returns HTTP 401 Unauthorized. |

---

## 6. Known Limitations & Failure Modes

1. **External Process Zombie Risk on Abrupt Power Loss:** While `ChildGuard` handles graceful SIGTERM/SIGINT teardown, abrupt machine power termination leaves process management to OS init.
2. **Model Non-Determinism:** Model latency and token rate-limiting from upstream providers can affect task execution speed. Axonel handles this via timeouts and retries, but extreme provider throttling will exhaust the mission budget.
3. **Compilation Lock Contention:** If multiple concurrent missions share the same Cargo build target cache, Cargo's internal lock files (`target/.package-cache`) can cause serialization. Dedicated worktrees with isolated target caches or workspace partitioning are recommended.

---

## 7. Remaining Work for Public v1.0 GA

1. **Packaging & Distribution:**
   - Single-binary distribution with embedded Web UI assets (`cargo embed` / `rust-embed`).
   - Homebrew formula and Debian `.deb` packaging.
2. **Expanded Agent Backend Adapters:**
   - Official adapter for Anthropic's Claude Code CLI (`claude`).
   - Official adapter for OpenAI Codex / Aider CLI.
3. **Multi-Platform CI Matrix:**
   - Continuous verification across macOS (Apple Silicon) and Linux Ubuntu 22.04/24.04.
4. **Telemetry & Structured Export:**
   - OpenTelemetry tracing exporter for enterprise supervisor deployments.

---

## 8. Release Candidate Sign-Off

The Axonel v0.1.0-rc1 candidate satisfies all functional, governance, and reliability requirements:
- **Transactional Integration & Reliability Matrix (Milestone 20):** 15 out of 15 Integration Reliability Scenarios passed (A through O) with 100% pass rate, verifying `Integrating` intermediate state, `WorkspaceLockManager`, Git-authoritative crash recovery, and real Gemini E2E integration.
- **Human Acceptance & Release Semantics Matrix (Milestone 19):** 15 out of 15 Release Semantics Scenarios passed (A through O).
- **Core Architecture Test Matrix (Milestone 18):** 15 out of 15 Release Candidate Matrix Scenarios passed (A through O).
- **Validation Integrity:** 6 independent validation runs executed across 3 diverse engineering workloads with 100% verification and integration success.
- **Codebase Integrity:** `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace` pass with 0 errors and 0 warnings.
- **Frontend Integrity:** TypeScript check and Vite production build pass cleanly.
