# Milestone 18: Validation Integrity + Release Candidate Hardening

**Milestone Status:** Completed & Verified  
**Date:** September 19, 2026  
**Artifacts Generated:**
- `web/tests/rc_matrix_tests.mjs`: Automated 15-scenario Release Candidate Matrix test suite.
- `web/tests/e2e_honest_validation.mjs`: Multi-run honest product validation harness (2 runs per workload).
- `docs/RELEASE_CANDIDATE.md`: Formal v1 release candidate specification and operating boundaries.
- `docs/PRODUCT_VALIDATION_RESULTS.md`: Updated empirical validation results with tempered, honest metrics.
- `/tmp/m18_validation_results.json`: Raw machine-readable benchmark telemetry.

---

## 1. Executive Summary

Milestone 17 established the first working product vertical for Axonel (autonomous tech-debt and defect remediation), but its experimental comparison suffered from a significant methodological flaw: Baseline A (raw Gemini CLI) was invoked with improper command-line arguments that caused the agent to exit prematurely in interactive mode, creating an artificially distorted baseline (0% success).

The primary objectives of Milestone 18 were:
1. **Validation Integrity:** Audit the validation methodology, eliminate unfair baselines, and execute an honest, multi-run benchmark (at least 2 runs per workload) where both Baseline A and Baseline B receive identical models, prompts, tools, and budgets.
2. **True Product Value Measurement:** Shift the evaluation from false claims of model cognitive superiority to measured developer responsibility (monitoring, diagnosis, independent testing, git hygiene, and integration).
3. **Substrate Hardening:** Enforce strict state invariants across the mission lifecycle, reject dirty target branches, cleanly abort on merge conflicts, sanitize sensitive credentials across all streams, and confine filesystem access.
4. **Release Candidate Test Matrix:** Design and pass an exhaustive automated test matrix covering Scenarios A through O.

All objectives were achieved. The 15-scenario Release Candidate Matrix passed with **100% success (15/15)**, and the multi-run validation suite confirmed Axonel's true value proposition: eliminating developer intervention while guaranteeing out-of-band verification and clean repository hygiene.

---

## 2. Post-Mortem on Milestone 17 Methodology

An exhaustive audit of `web/tests/e2e_milestone17.mjs` revealed several critical flaws in the original experimental methodology:

### A. The CLI Invocation Flaw
In M17, Baseline A was invoked via:
```javascript
execFileSync("gemini", ["--model", "gemini-3.1-flash-lite", "-y", prompt], { stdio: "ignore" });
```
- **Positional Prompt Interpretation:** Without the `-p` flag, the Gemini CLI treats the prompt argument as a positional command and expects an interactive terminal session.
- **Immediate EOF Exit:** Because `stdio: "ignore"` closed standard input, the CLI immediately received `EOF` and terminated within 4 seconds without reading files or executing tools.
- **Directory Trust Prompt:** The `--skip-trust` flag was missing, causing non-interactive runs to abort on directory security checks.
- **Approval Mode:** The `-y` flag is non-standard in current Gemini CLI versions; the correct headless flag is `--approval-mode yolo`.

### B. Single-Run Noise
Milestone 17 evaluated each workload only once, making it vulnerable to model stochasticity, prompt variance, and network jitter.

### C. The Corrected Baseline (Milestone 18)
In Milestone 18, Baseline A was invoked with the exact headless flags utilized by Axonel's internal `GeminiCliBackend`:
```javascript
execFileSync("gemini", [
  "-p", objective,
  "-m", "gemini-3.1-flash-lite",
  "--skip-trust",
  "--approval-mode", "yolo"
], { cwd: dir, stdio: "ignore", timeout: 180000 });
```
Under this fair configuration, Baseline A successfully inspected files, wrote code fixes, and verified compilation on all workloads.

---

## 3. Honest Multi-Run Validation Results

We evaluated 3 representative engineering workloads across 2 independent runs for both Baseline A (raw Gemini CLI) and Baseline B (Axonel Supervised Mission), yielding 12 total agent executions.

### Workload Definitions
1. **Workload 1 (`concurrency_gate`):** Failing concurrency test due to a check-then-act race condition in `try_acquire`.
2. **Workload 2 (`api_gateway`):** Breaking API migration causing compiler error under `#![deny(warnings)]` due to missing `RouteStatus::Archived` match arm.
3. **Workload 3 (`query_parser`):** Missing URL percent-decoding (`%20` $\to$ space, `%2B` $\to$ `+`) failing unit tests.

### Empirical Results Table

| Workload | Run | Baseline A (Raw Agent) Duration | Baseline A Verified? | Baseline A Dirty Git? | Baseline A Dev Actions | Baseline B (Axonel) Duration | Axonel Cycles | Axonel Verified? | Axonel Integrated? | Axonel Dev Actions |
|---|---|---|---|---|---|---|---|---|---|---|
| **W1: Concurrency Gate** | Run 1 | 43s | ✅ Yes | ❌ Dirty | 4 | **46s** | 2 | ✅ Yes (`5512bb6...`) | ✅ Yes | **0** |
| | Run 2 | 27s | ✅ Yes | ❌ Dirty | 4 | **32s** | 2 | ✅ Yes (`d542a53...`) | ✅ Yes | **0** |
| **W2: API Gateway** | Run 1 | 48s | ✅ Yes | ❌ Dirty | 4 | **33s** | 2 | ✅ Yes (`ce99c5f...`) | ✅ Yes | **0** |
| | Run 2 | 35s | ✅ Yes | ❌ Dirty | 4 | **46s** | 2 | ✅ Yes (`49f0e9e...`) | ✅ Yes | **0** |
| **W3: Query Parser** | Run 1 | 40s | ✅ Yes | ❌ Dirty | 4 | **39s** | 2 | ✅ Yes (`d8a3daa...`) | ✅ Yes | **0** |
| | Run 2 | 40s | ✅ Yes | ❌ Dirty | 4 | **43s** | 2 | ✅ Yes (`a95778c...`) | ✅ Yes | **0** |
| **Averages / Totals** | | **38.8s** | **6/6 (100%)**| **100% Dirty** | **4 Actions** | **39.8s** | **2.0** | **6/6 (100%)** | **6/6 (100%)**| **0 Actions** |

### What the Empirical Evidence Actually Shows

1. **Model Capability Parity:**
   When properly invoked, Google Gemini 3.1 Flash Lite solves all three algorithmic and syntactic tasks (6/6 success in Baseline A and Baseline B). Axonel makes no claim of creating a "smarter" model.

2. **The True Developer Burden of Raw Agents:**
   Although the raw agent solves the problem, it imposes substantial developer overhead:
   - **Manual Monitoring:** The developer must poll or monitor the process terminal to determine when execution halts.
   - **Dirty / Uncommitted Working Trees:** The raw agent modifies the active repository directly, generating untracked files (`Cargo.lock`, build caches) and leaving changes uncommitted or unstaged.
   - **Manual Verification:** The developer cannot trust agent self-reports and must manually run `cargo test` out-of-band.
   - **Manual Git Integration:** The developer must manually create a branch, write a commit message, and merge or submit a PR.
   - **Total developer actions required: 4 per task.**

3. **The Measured Value of Axonel:**
   Axonel converts raw LLM capabilities into an autonomous engineering pipeline:
   - **Zero Developer Actions:** The developer dispatches a mission with an objective prompt and walks away (**0 actions**).
   - **Worktree Isolation:** The developer's primary workspace is never touched or polluted during execution.
   - **Independent Compiler Verification:** Axonel refuses to mark any mission complete until physical on-disk stopping conditions (`cargo test = 0`, clean working tree, commit exists) are satisfied.
   - **Safe Git Integration:** Axonel merges the verified commit into the target branch or cleanly aborts with HTTP 409 if a conflict occurs.
   - **Compute Overhead:** Axonel adds negligible wall-clock overhead (~1s on average: 39.8s vs 38.8s) while providing full multi-cycle supervision, crash recovery, and provenance.

---

## 4. Release Candidate Test Matrix (Scenarios A through O)

The automated suite `web/tests/rc_matrix_tests.mjs` validates 15 safety and lifecycle scenarios against the live HTTP control plane:

| Code | Scenario | Status | Verified Outcome / Behavior |
|---|---|---|---|
| **A** | Happy-path mission with real Gemini CLI | **PASS** | Completed in 2 cycles; verified out-of-band at SHA `ae7b7775`; integrated to `main`. |
| **B** | Agent timeout & budget circuit breaker | **PASS** | Enforced `max_executions: 0` limit; halted in `budget_exhausted` state. |
| **C** | Agent crash / task error handling | **PASS** | Captured syntax error in crate; initiated recovery and replanned to Cycle 1. |
| **D** | Verification failure rejection | **PASS** | Refused to mark mission completed when `custom_verifier` returned exit code 1. |
| **E** | Adaptive recovery & replanning cycle | **PASS** | Successfully synthesized replacement workflow and advanced to Cycle 2. |
| **F** | Server restart state restoration | **PASS** | Killed control plane with `SIGTERM`; restored intact from SQLite with identical state. |
| **G** | Mission administrative cancellation | **PASS** | Cancelled active mission via API; subsequent step calls rejected as terminal. |
| **H** | Mission pause and resume lifecycle | **PASS** | Paused mission to `Waiting` state; resumed to `Running` state without data loss. |
| **I** | Dirty target branch rejection | **PASS** | Integration refused with HTTP 409 Conflict when target tree had uncommitted files. |
| **J** | Merge conflict integration rejection | **PASS** | 3-way merge conflict detected; executed `git merge --abort`; returned HTTP 409 Conflict. |
| **K** | Unverified integration rejection | **PASS** | Integration rejected with HTTP 409 Conflict on mission in `Created` unverified state. |
| **L** | Secret redaction in events & logs | **PASS** | Google API keys, Bearer tokens, and AWS keys redacted in event stream and terminal buffer. |
| **M** | Worktree boundary escape protection | **PASS** | Path traversal outside sandbox boundary rejected with confinement error. |
| **N** | Concurrent missions isolation | **PASS** | Two simultaneous missions executed in distinct workspaces without cross-contamination. |
| **O** | Duplicate execution prevention | **PASS** | Stepping completed mission was a no-op; terminal state invariants preserved. |

---

## 5. Substrate Hardening Summary

During Milestone 18, key safety improvements were implemented in the codebase:

1. **Git Integration Engine (`crates/plexis-server/src/git.rs`):**
   - Implemented `integrate_git_commit` using native git operations.
   - Pre-flight check rejects dirty target working trees (`git status --porcelain`).
   - Resolves commit object in local object store (`git cat-file -e`).
   - Fast-forward merge attempted first; falls back to 3-way merge (`git merge --no-ff`).
   - Merge conflict handler executes `git merge --abort`, ensuring target trees are never left in an unmerged or dirty state.

2. **Route Integration Hardening (`crates/plexis-server/src/routes.rs`):**
   - `integrate_mission` rejects non-verified or uncompleted missions with HTTP 409 Conflict.
   - Returns clear structured JSON errors on dirty branches and merge conflicts.

3. **Automated Secret Redaction (`crates/plexis-tools/src/redaction.rs`):**
   - Added regex pattern `\bAIza[0-9A-Za-z_\-]{20,50}\b` for Google API keys.
   - Added `SecretRedactor::redact_value_all` to `emit_mission_event` in `crates/plexis-runtime/src/mission/engine.rs`, ensuring all persisted mission event payloads are automatically sanitized.

4. **Serde Deserialization Robustness (`crates/plexis-core/src/mission.rs`):**
   - Added `#[serde(default)]` and aliases (`max_duration_seconds`, `max_total_executions`) to `MissionBudget` and `StoppingCondition`, preventing unprocessable entity errors when clients provide partial budget payloads.

---

## 6. Release Readiness Sign-Off

- **Code Quality:** `cargo fmt --check` passes cleanly.
- **Linter:** `cargo clippy --workspace --all-targets --all-features` passes with 0 warnings.
- **Unit & Integration Tests:** 21 server integration tests and 100+ workspace tests pass with 0 failures.
- **Empirical Validation:** 100% verified execution across all benchmark runs.
- **Safety Boundaries:** 15/15 matrix scenarios pass.

Axonel is ready for Release Candidate packaging as defined in `docs/RELEASE_CANDIDATE.md`.
