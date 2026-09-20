# Axonel Validation & Verification Receipts

Axonel adheres to strict empirical verification standards. Every architectural claim, security boundary, and reliability guarantee in Axonel is backed by automated test suites and reproducible benchmarks.

This document summarizes the validation receipts for Axonel v0.1.1.

---

## 1. Test Suite Summary

| Test Layer | Test Suite / Harness | Assertions / Cases | Pass Rate | Invariants Verified |
| :--- | :--- | :--- | :--- | :--- |
| **Workspace Unit Tests** | `cargo test --workspace` | 74 test cases | **100%** (74/74) | Domain types, SQLite WAL storage, tool containment, PGID handling, parser logic |
| **Code Formatting** | `cargo fmt --all -- --check` | Entire workspace | **100%** | Zero formatting deviations |
| **Static Analysis** | `cargo clippy --workspace --all-targets -- -D warnings` | All crates | **100%** | Zero clippy warnings across all workspace crates |
| **Concurrency Stress** | `test_concurrent_command_claiming` | 20 concurrent threads | **100%** (20/20) | Zero race conditions on command lease claims |
| **Security Gates** | `crates/plexis-server/tests/security_tests.rs` | 4 test cases | **100%** (4/4) | Loopback bind default, rejected external bind without auth, auth token validation |
| **Safety Regression** | `quickstart_safety_regression_test.rs` | Integration test | **100%** | Main branch protection, worktree isolation, refusal of unverified merges |
| **Browser E2E** | `web/tests/e2e_release_candidate.mjs` (Playwright) | 15 assertions | **100%** (15/15) | Full mission lifecycle, diff viewer, review package modal, operator acceptance |
| **Adversarial Audit** | `m20_integration_reliability_tests.mjs` | 15 scenarios | **100%** (15/15) | Crash recovery (`SIGKILL`), merge conflict abort, dirty working tree protection |
| **Real Gemini CLI** | `web/tests/e2e_honest_validation.mjs` (M18/M19) | 12 empirical runs | **100%** (12/12) | Real Gemini CLI agent autonomously fixing code in isolated worktrees |
| **Continuous Integration** | GitHub Actions (`.github/workflows/ci.yml`) | Remote pipeline | **100% GREEN** | Clean build, test, format, and clippy passes on remote runner |

---

## 2. Empirical Benchmark: Raw Agent vs. Axonel Supervisor (M18)

In Milestone 18, we benchmarked direct headless execution of Google Gemini CLI against Axonel's supervised execution across 3 standard coding workloads (12 total runs):

| Metric | Raw Gemini CLI (Baseline A) | Axonel Supervised (Baseline B) |
| :--- | :--- | :--- |
| **Test Pass Rate** | 6/6 (100%) | 6/6 (100%) |
| **Primary Working Tree State** | **100% Dirty** (untracked files, build artifacts) | **100% Clean** (isolated worktrees) |
| **Manual Developer Actions** | **4 actions per run** (monitor, clean, test, commit) | **0 actions required** (autonomous supervision) |
| **Verification Authority** | Agent self-report (unverified) | **Independent physical disk verification** |
| **Average Wall-Clock Duration** | 38.8s | 39.8s (~1s supervisor overhead) |

For complete benchmark methodology and per-workload telemetry, see [PRODUCT_VALIDATION_RESULTS.md](PRODUCT_VALIDATION_RESULTS.md).

---

## 3. Adversarial & Crash Recovery Receipts (M20)

During the M20 adversarial audit, Axonel was subjected to deliberate crash and fault-injection scenarios:

1. **Mid-Flight Termination (`SIGKILL`):**
   - The supervisor daemon was forcibly terminated with `SIGKILL` while a mission was in the `Integrating` state.
   - Upon restart, Axonel performed authoritative Git ancestry recovery (`git merge-base --is-ancestor`) and reconciled state with zero data corruption.
2. **Merge Conflict Abort:**
   - A conflicting commit was pushed to the target branch while an agent was working in its worktree.
   - Axonel detected the conflict during the integration phase, rejected the merge with HTTP 409 Conflict, and atomically rolled back the mission state to `Accepted` without dirtying the repository.
3. **Dirty Primary Tree Defense:**
   - Axonel refused to integrate candidate commits when the primary working tree contained unstaged changes, preventing developer data loss.

For detailed test logs and reproduction commands, see [REAL_WORLD_VALIDATION_RESULTS.md](REAL_WORLD_VALIDATION_RESULTS.md) and [GO_NO_GO.md](GO_NO_GO.md).
