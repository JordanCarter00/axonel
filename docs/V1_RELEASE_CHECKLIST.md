# Axonel Public v1 Release Candidate Checklist

**Status:** Certified Release Candidate (RC1)  
**Date:** September 19, 2026  
**Target Release:** Axonel v0.1.0  
**Operating Boundary:** Local-First Linux x86_64 (`x86_64-unknown-linux-gnu`; aarch64 planned/unverified)  

This checklist certifies Axonel's operational readiness, evidence integrity, packaging, and security invariants for its first public v1 release. Every criterion is verified by automated test suites, real browser flows, empirical validation telemetry, or clean source builds.

---

## 1. Security & Network Boundary

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Default Loopback Binding** | Axonel server defaults strictly to `127.0.0.1` (loopback). Rejects binding to non-loopback interfaces without explicit authentication. | `crates/plexis-server/tests/security_tests.rs::test_a_default_bind_loopback_auth_status` | **PASS** |
| **Non-Loopback Auth Enforcement** | Binding to `0.0.0.0` or public interfaces without `--auth-token` or `AXONEL_AUTH_TOKEN` fails server startup immediately with exit code 1. | `crates/plexis-server/tests/security_tests.rs::test_b_non_loopback_without_auth_fails_startup` | **PASS** |
| **Protected API Authentication** | When authentication is enabled, all protected routes (`/api/v1/*`) require valid `Authorization: Bearer <token>`; missing or invalid tokens return HTTP 401 Unauthorized. | `crates/plexis-server/tests/security_tests.rs::test_e_auth_enforcement_on_protected_endpoints` | **PASS** |
| **Truthful Auth Status** | The public `/api/v1/auth/status` endpoint truthfully reports `auth_required: true` and `loopback_only: false` when bound to external interfaces, preventing UI spoofing. | `crates/plexis-server/tests/security_tests.rs::test_f_auth_status_never_claims_loopback_when_non_loopback` | **PASS** |
| **Secret Sanitization** | Sensitive credentials (Bearer tokens, OpenAI/Anthropic keys, Google API keys, GitHub tokens, AWS keys, PEM certificates) are automatically redacted in logs and event streams. | `crates/plexis-tools/tests/secret_redaction_and_backend_tests.rs` | **PASS** |

---

## 2. Workspace & Worktree Confinement

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Worktree Isolation** | Autonomous coding missions execute exclusively in isolated Git worktrees (`.plexis/worktrees/<task_id>`); the developer's main branch and working tree are untouched during execution. | `crates/plexis-runtime/tests/multi_agent_worktree_tests.rs` | **PASS** |
| **Path Traversal Confinement** | File operations outside the designated workspace root (e.g. `../`, `/etc/passwd`) are detected and rejected with `ToolError::ConfinementViolation`. | `crates/plexis-tools/tests/sandbox_escape_tests.rs` | **PASS** |
| **Subprocess Confinement** | External agent processes are launched in dedicated OS process groups (`setpgid`) with bounded wall-clock, token, and turn limits; orphaned processes are reaped on termination. | `crates/plexis-runtime/tests/agent_host_supervision_tests.rs` | **PASS** |

---

## 3. Independent Verification & Stopping Invariants

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Physical Disk Verification** | Agent claims of success are untrusted. Verification executes directly on disk using authoritative toolchains (`cargo test`, `npm test`, `pytest`). | `crates/plexis-runtime/tests/mission_execution_tests.rs` | **PASS** |
| **Clean Working Tree Invariant** | Verification fails if the working tree contains uncommitted modifications or untracked build artifacts (`working_tree_clean == true`). | `docs/validation/results.json` (Baseline B & Long Horizon records) | **PASS** |
| **Authoritative Commit Existence** | Stopping condition requires an explicit Git commit object to exist in the repository (`required_commit_exists == true`). | `crates/plexis-runtime/tests/mission_execution_tests.rs` | **PASS** |
| **Adaptive Replanning** | When stopping conditions fail or stagnation is detected, the supervisor triggers multi-turn recovery cycles rather than stalling indefinitely. | `crates/plexis-runtime/tests/recovery_controller_tests.rs` | **PASS** |

---

## 4. Human Governance & Explicit Acceptance Gate

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Halts at AwaitingAcceptance** | Autonomous missions NEVER automatically merge into the target branch upon verification. Execution halts at `AwaitingAcceptance`. | `web/tests/m19_acceptance_tests.mjs` | **PASS** |
| **Unaccepted Integration Blocked** | Direct calls to `/api/v1/missions/{id}/integrate` before explicit acceptance are rejected with HTTP 409 Conflict. | `web/tests/m19_acceptance_tests.mjs` & `web/tests/e2e_release_candidate.mjs` | **PASS** |
| **Structured Review Package** | `GET /api/v1/missions/{id}/review` provides unified diff, changed files list, verification receipts, and HEAD freshness indicators (`reverification_required`). | `web/tests/m19_acceptance_tests.mjs` | **PASS** |
| **Explicit Acceptance Action** | Operators can explicitly accept (`POST /api/v1/missions/{id}/accept`) with or without immediate integration. | `web/tests/e2e_release_candidate.mjs` | **PASS** |
| **Non-Destructive Rejection** | Rejection requires a recorded reason, transitions mission to `Rejected`, and preserves worktrees and candidate commits on disk for inspection. | `web/tests/m19_acceptance_tests.mjs` | **PASS** |

---

## 5. Crash-Safe Integration & State Reconciliation

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Durable SQLite Persistence** | All missions, cycles, leases, and events are stored in transactional SQLite with WAL mode and foreign key constraints. | `crates/plexis-storage/tests/sqlite_storage_tests.rs` | **PASS** |
| **Intermediate Integrating State** | The control plane logs durable intent (`integration_intent`) before touching physical Git state. | `crates/plexis-runtime/tests/command_reconciliation_tests.rs` | **PASS** |
| **Git Ancestry Authoritative Recovery** | Upon startup crash reconciliation, the engine queries `git merge-base --is-ancestor`. If commit is merged -> `Integrated`; if not -> resets cleanly to `Accepted`. | `web/tests/m20_integration_reliability_tests.mjs` | **PASS** |
| **Pristine Rollback on Conflict** | Merge conflicts trigger immediate `git merge --abort` and reset to `Accepted` with HTTP 409 Conflict, leaving the target branch untouched. | `web/tests/m20_integration_reliability_tests.mjs` | **PASS** |
| **Target Branch Freshness Protection** | Integration checks current target branch HEAD against `expected_target_head` to prevent overwriting concurrent commits. | `web/tests/m20_integration_reliability_tests.mjs` | **PASS** |

---

## 6. Packaging, Build & Developer Experience

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Single Linux Binary Build** | Axonel compiles cleanly into a unified binary (`axonel`) via `cargo build --release -p plexis-server --bin axonel`. | Release build verification (`target/release/axonel --help`) | **PASS** |
| **Web UI Asset Distribution** | Web assets build to `web/dist` and are served automatically by Axonel on loopback default (`127.0.0.1:3000`). | `npm --prefix web run build` | **PASS** |
| **Clean First-Run Flow** | First-run experience (`Install -> Start -> Register -> Create -> Watch -> Review -> Accept -> Integrate`) uses user-centric domain terms without internal crate jargon. | `README.md` & `docs/REPRODUCIBLE_RELEASE.md` | **PASS** |
| **CLI Usability & Help Surface** | Top-level CLI commands (`init`, `serve`, `status`, `mission`, `reconcile`) provide clear, actionable help output. | `axonel --help` / `axonel serve --help` | **PASS** |

---

## 7. Real Browser & Real-World Validation Evidence

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Real Browser Release E2E** | Complete 15-assertion browser test via Playwright Chromium executing the full lifecycle from repository registration to on-disk verification. | `web/tests/e2e_release_candidate.mjs` (15/15 passed) | **PASS** |
| **Real-World Empirical Dataset** | 20-task benchmark corpus defined in `docs/validation/dataset.json` with explicit `PLANNED` status. | `docs/validation/dataset.json` | **PASS** |
| **Empirical Telemetry Integrity** | Exact machine-readable records in `docs/validation/results.json` matching observed telemetry across Rust, TypeScript, and Python without fabricated claims. | `docs/validation/results.json` | **PASS** |
| **Fair Baseline Comparison** | Direct Gemini CLI vs. Axonel compared under strictly identical permissions, prompts, and verification criteria. | `docs/REAL_WORLD_VALIDATION_RESULTS.md` | **PASS** |

---

## 8. Continuous Integration & Quality Gates

| Criterion | Requirement | Verification Method | Status |
| :--- | :--- | :--- | :--- |
| **Zero Compiler Warnings** | Entire Rust workspace compiles with zero warnings under `cargo clippy --all-targets -- -D warnings`. | Local & CI Clippy check | **PASS** |
| **Workspace Test Suite** | All unit and integration test suites pass cleanly across all crates. | `cargo test --workspace` | **PASS** |
| **Remote CI Green** | GitHub Actions workflow `.github/workflows/ci.yml` is 100% green on remote `origin/main`. | GitHub Actions Run `3543597...` | **PASS** |
| **Zero P0 Release Blockers** | No critical security vulnerabilities, data loss paths, or unhandled panics in release flows. | Milestone 23 audit | **PASS** |

---

## Certification Conclusion

Axonel v0.1.0-rc1 satisfies all 18 release criteria with explicit **PASS** status. The system is certified ready for public release.
