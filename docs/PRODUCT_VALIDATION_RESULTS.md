# Product Validation Results: Autonomous Engineering Supervisor

**Milestone:** 18 (Corrected & Multi-Run Hardened)  
**Harness:** `web/tests/e2e_honest_validation.mjs`  
**Execution Timestamp:** 2026-09-19T06:46:52Z  
**Runtime Environment:** Linux x86_64, Rust 1.88+, Google Gemini CLI (`gemini-3.1-flash-lite`)  
**Methodology Standard:**
- Both baselines receive identical initial repository states, objectives, models, and budgets.
- Baseline A invoked with correct headless flags: `gemini -p <prompt> -m gemini-3.1-flash-lite --skip-trust --approval-mode yolo`.
- Two independent runs executed per workload (6 runs per baseline, 12 total runs).
- Zero source code injection by test harness; independent physical verification executed out-of-band via `cargo test`.

---

## 1. Executive Summary

Milestone 18 establishes an honest, rigorous empirical comparison between:
- **Baseline A (Raw Gemini CLI Agent):** Direct headless execution of the external agent in the repository with real tool permissions and zero supervisor infrastructure.
- **Baseline B (Axonel Autonomous Mission):** Autonomous execution in an isolated Git worktree under control-plane supervision, multi-cycle recovery, independent out-of-band compiler verification, and safe branch integration.

### Core Empirical Findings:
1. **Model Capability Parity:** Under fair, non-interactive flags, Google Gemini 3.1 Flash Lite resolves localized coding defects in both baselines (100% test pass rate in both Baseline A and Baseline B). The model itself is capable of solving the coding problems.
2. **The Real Developer Tax of Raw Agents:**
   - In 100% of raw agent runs, the active repository was left in a dirty state with unstaged edits and untracked build artifacts.
   - The developer had to perform **4 distinct manual actions per run** (monitor process completion, inspect and clean git status, execute manual verification tests, and create commits/branches).
3. **The Autonomous Supervisor Advantage of Axonel:**
   - **0 developer actions required:** Axonel manages the mission from objective dispatch to target branch merge completely unattended.
   - **Worktree Isolation:** The developer's primary workspace is never touched or polluted during execution.
   - **Independent Verification:** Axonel refuses completion until physical disk verifiers (`cargo test = 0`) pass out-of-band.
   - **Safe Git Integration:** Axonel integrates verified commits into target branches and cleanly aborts on merge conflicts (HTTP 409 Conflict) without leaving repositories dirty.
   - **Overhead:** Axonel adds virtually zero wall-clock overhead (~1s on average: 39.8s vs 38.8s).

---

## 2. Empirical Benchmark Telemetry

| Workload ID & Name | Run | Baseline A Duration | Baseline A Verified? | Baseline A Dirty Working Tree? | Baseline A Dev Actions | Baseline B (Axonel) Duration | Axonel Cycles | Axonel Verified Commit | Axonel Integrated? | Axonel Dev Actions |
|---|---|---|---|---|---|---|---|---|---|---|
| **W1: Concurrency Gate** (`concurrency_gate`) | Run 1 | 43s | ✅ Yes | ❌ Dirty | 4 | **46s** | 2 | `5512bb65...` | ✅ Yes | **0** |
| | Run 2 | 27s | ✅ Yes | ❌ Dirty | 4 | **32s** | 2 | `d542a537...` | ✅ Yes | **0** |
| **W2: API Gateway** (`api_gateway`) | Run 1 | 48s | ✅ Yes | ❌ Dirty | 4 | **33s** | 2 | `ce99c5f2...` | ✅ Yes | **0** |
| | Run 2 | 35s | ✅ Yes | ❌ Dirty | 4 | **46s** | 2 | `49f0e9e3...` | ✅ Yes | **0** |
| **W3: Query Parser** (`query_parser`) | Run 1 | 40s | ✅ Yes | ❌ Dirty | 4 | **39s** | 2 | `d8a3daa7...` | ✅ Yes | **0** |
| | Run 2 | 40s | ✅ Yes | ❌ Dirty | 4 | **43s** | 2 | `a95778ce...` | ✅ Yes | **0** |
| **Averages / Totals** | | **38.8s** | **6/6 (100%)** | **100% Dirty** | **4 Actions** | **39.8s** | **2.0** | **6/6 Verified** | **6/6 (100%)** | **0 Actions** |

---

## 3. Workload Analysis & Developer Responsibility

### Workload 1: Concurrency Gate Race Condition Fix (`concurrency_gate`)
- **Bug:** Unsynchronized check-then-act in `try_acquire` allowed concurrent threads to exceed capacity.
- **Baseline A:**
  - Gemini CLI correctly implemented atomic compare-and-swap logic.
  - However, the agent ran directly in the repository, creating untracked files (`Cargo.lock`, `target/`). The developer had to manually test, review unstaged files, commit, and merge.
- **Baseline B (Axonel):**
  - Axonel isolated execution in a dedicated worktree.
  - Cycle 0 performed requirements discovery and defect diagnosis; Cycle 1 implemented the thread-safe logic and committed.
  - Physical out-of-band verification passed on disk. Integrated into `main` with 0 human interventions.

### Workload 2: API Gateway Compiler Warning Repair (`api_gateway`)
- **Bug:** Enum variant `RouteStatus::Archived` added to enum under `#![deny(warnings)]`, causing compilation failure.
- **Baseline A:**
  - Gemini CLI added the missing match arm. Developer had to verify out-of-band and clean git state.
- **Baseline B (Axonel):**
  - Autonomous mission executed across 2 cycles, validated compiler exit code 0 on host disk, and merged cleanly.

### Workload 3: URL Query Parser Percent Decoding (`query_parser`)
- **Bug:** Query string parser failed to decode percent-encoded hex sequences (`%20` and `%2B`).
- **Baseline A:**
  - Gemini CLI implemented hex parsing and string replacement. Developer had to inspect and merge manually.
- **Baseline B (Axonel):**
  - Axonel executed the multi-agent workflow, verified passing tests on disk, and performed atomic merge integration.

---

## 4. Summary of Measured Product Value

```text
+----------------------------------------------------------------------------------------------------+
|                                    DEVELOPER RESPONSIBILITY                                        |
+------------------------------------+----------------------------------+----------------------------+
| Lifecycle Stage                    | Raw Coding Agent (Baseline A)    | Axonel Supervisor (Base B) |
+------------------------------------+----------------------------------+----------------------------+
| 1. Process Supervision             | Developer monitors terminal      | Autonomous daemon          |
| 2. Working Tree Safety             | Active repository exposed        | Isolated Git worktree      |
| 3. Verification Trust              | Agent self-report (unverified)   | Independent disk verifier  |
| 4. Failure Recovery                | Developer manually re-prompts    | Multi-cycle adaptive replan|
| 5. Git Integration                 | Developer commits and merges     | Safe atomic integration    |
| 6. Merge Conflict Protection       | Unhandled; can corrupt git tree  | Aborted safely (HTTP 409)  |
| 7. Human Actions Required          | 4 actions per task               | 0 actions per task         |
+------------------------------------+----------------------------------+----------------------------+
```

---

## 5. Methodological Audit & Correction Post-Mortem

In Milestone 17, Baseline A was reported as achieving 0% success (0/3). Our Milestone 18 audit revealed that Baseline A had been launched without the `-p` prompt flag, with closed standard input (`stdio: "ignore"`), and without `--approval-mode yolo --skip-trust`. This caused the CLI to terminate immediately upon EOF rather than failing due to cognitive limitations.

By correcting these flags in Milestone 18 and running multiple independent repetitions, we established that:
- The raw Gemini 3.1 Flash Lite model is capable of solving localized coding tasks.
- The value of Axonel does not rely on claiming the underlying model is incompetent.
- The value of Axonel is **eliminating the developer overhead of supervising, verifying, recovering, and integrating coding agents**, turning an interactive tool into an autonomous background workflow.
