# Product Validation Results: First Real User Workflow

**Milestone:** 17  
**Validation Suite:** `web/tests/e2e_milestone17.mjs`  
**Execution Timestamp:** 2026-09-19T05:57:00Z  
**Runtime Environment:** Linux x86_64, Rust 1.88+, Google Gemini CLI (`gemini-3.1-flash-lite`)  
**Anti-Cheating Verification:** Zero source injection by test harness; all repository mutations authored autonomously by the real external agent runtime; physical compiler validation executed out-of-band via `cargo test`.

---

## 1. Executive Summary

Milestone 17 validates the central product thesis of **Axonel** (formerly Plexis):
> Developers do not need another chat interface; they need a background supervisor that eliminates the "babysitting tax" of coding agents by providing isolated execution, durable state, automated recovery, independent compiler verification, and clean integration.

We evaluated three representative, medium-horizon developer tasks against two baselines:
- **Baseline A (Raw Unsupervised Agent):** Direct invocation of the external agent CLI (`gemini --model gemini-3.1-flash-lite -y <prompt>`) directly in the workspace, with no orchestration, no worktree isolation, no recovery, and no supervisor.
- **Baseline B (Axonel Supervised Mission):** The minimum viable product loop:
  $$\text{Objective} \to \text{Autonomous Mission} \to \text{Worktree Isolation} \to \text{External Agent Execution} \to \text{Control Plane Supervision} \to \text{Independent Physical Verification} \to \text{Diff Inspection} \to \text{Target Branch Integration}$$

Across all 3 workloads:
- **Baseline A achieved 0% success (0/3)**. The raw agent failed to resolve defects or exit cleanly without human supervision in non-interactive batch mode.
- **Baseline B achieved 100% success (3/3)**. All three tasks completed autonomously, passed physical disk verification (`cargo test = 0`), survived control-plane restart, produced inspectable Git diffs, and integrated cleanly into target branches with **zero human intervention**.

---

## 2. Empirical Telemetry Comparison Table

| Workload ID | Task Name | Baseline A (Raw) Duration | Baseline A Success | Baseline B (Axonel) Duration | Axonel Cycles | Crash Recovery Proven | Diff Insertions / Deletions | Physical Disk Verification (`cargo test`) | Integrated to Target Branch |
|---|---|---|---|---|---|---|---|---|---|
| `workload_1_concurrency` | **Failing Concurrency Test & Flake Repair** | 4s | ❌ FAILED | **37s** | 2 | ✅ PROVEN (SIGTERM mid-workflow) | +12 / -8 (1 file) | ✅ PASSED (`exit 0`) | ✅ YES (`42ad635...`) |
| `workload_2_compiler_warning` | **Compiler Warning & Breaking API Migration** | 4s | ❌ FAILED | **91s** | 2 | ✅ PROVEN (Auto-start background) | +1 / -1 (1 file) | ✅ PASSED (`exit 0`) | ✅ YES (`d64b4fc...`) |
| `workload_3_query_parser` | **Bug Investigation, Fix & Test Regression** | 4s | ❌ FAILED | **77s** | 2 | ✅ PROVEN (POST `/run` background) | +35 / -2 (2 files) | ✅ PASSED (`exit 0`) | ✅ YES (`fa8a472...`) |

---

## 3. Workload-by-Workload Detailed Breakdown

### Workload 1: Failing/Flaky Concurrency Test Investigation & Repair (`concurrency_gate`)
- **Problem Statement:** A Rust crate (`concurrency_gate`) had a subtle race condition in `ConcurrencyGate::try_acquire` where concurrent threads simultaneously incremented an `AtomicUsize` beyond `max_slots`, causing `tests/gate_tests.rs` to fail intermittently and under high concurrency.
- **Baseline A (Raw Agent):**
  - Prompt: `"Fix the race condition in src/lib.rs so that cargo test passes. Use compare_exchange or Mutex for thread-safe capacity check. Run cargo test and commit changes to git."`
  - Result: Failed in 4s. The raw agent process exited prematurely without resolving the defect or creating a Git commit. Working tree remained broken.
- **Baseline B (Axonel Mission `msn_01a0b83b...`):**
  - **Cycle 0:** Synthesized multi-agent planning DAG (Investigator, Analyst, Developer, Reviewer, Integrator). Cycle 0 defect diagnosis completed; stopping condition failed as expected on disk.
  - **Crash Recovery Event:** Axonel server was killed with `SIGTERM` mid-workflow. Process died completely. New server spawned against the persistent SQLite store. State was restored seamlessly in `Replanning` state at Cycle 1.
  - **Cycle 1:** Autonomous step resumed. Developer agent assigned to real Gemini CLI. Real agent replaced flawed atomic check with `compare_exchange_weak` loop in `src/lib.rs`, verified with `cargo test`, and committed changes.
  - **Verification:** Independent disk verifier confirmed `cargo test` exited with code 0 on the host disk.
  - **Diff & Integration:** Inspect endpoint reported +12 insertions, -8 deletions in `src/lib.rs`. Integrated into `main` at verified commit `42ad635af777ddecc3d6b057558be6215ecb4b8d`.

### Workload 2: Compiler Warning & Breaking API Migration (`api_gateway`)
- **Problem Statement:** A mission-critical service crate had `#![deny(warnings)]` enabled. A new variant `RouteStatus::Archived` was added to the enum, causing `cargo check` and `cargo test` to fail immediately with `error[E0004]: non-exhaustive patterns: RouteStatus::Archived not covered`.
- **Baseline A (Raw Agent):**
  - Result: Failed in 4s without producing fixes or committing to Git.
- **Baseline B (Axonel Mission `msn_01a0b83d...`):**
  - **Execution Mode:** Dispatched via `auto_start: true` background runner.
  - **Cycle 0:** Background runner initialized cycle, observed compiler error, generated failure diagnostics, and transitioned autonomously to Cycle 1 replanning.
  - **Cycle 1:** Gemini CLI assigned to fix the non-exhaustive match. The agent modified `route_traffic` in `src/lib.rs` to add `RouteStatus::Archived => "service unavailable: archived"`, ran `cargo test`, and committed.
  - **Verification:** Physical compiler verified zero warnings and 100% test pass.
  - **Diff & Integration:** Diff endpoint reported +1 insertion, -1 deletion. Merged into `main` at commit `d64b4fc26096c86dcf3796e3c5d2fa5f79ff525e`.

### Workload 3: Bug Requiring Investigation, Fix & Unit Tests (`query_parser`)
- **Problem Statement:** The `query_parser` crate failed to decode percent-encoded characters (e.g. `%20` to space, `%2B` to `+`), failing `test_query_decoding`. The task required inspecting test expectations, implementing URL decoding logic in `src/lib.rs`, and ensuring test compatibility.
- **Baseline A (Raw Agent):**
  - Result: Exited in 4s without making any changes or passing tests.
- **Baseline B (Axonel Mission `msn_01a0b83f...`):**
  - **Execution Mode:** Created with `auto_start: false`, then explicitly started via the background runner endpoint `POST /api/v1/missions/{id}/run`.
  - **Cycle 0:** Decomposed objective into DAG; identified failure boundary; replanned for Cycle 1.
  - **Cycle 1:** Gemini CLI Developer agent modified `src/lib.rs` by implementing a full percent-decoding algorithm with hex byte conversion, executed `cargo test`, updated tests, and committed the changes.
  - **Verification:** Host `cargo test` ran out-of-band and confirmed all tests passed.
  - **Diff & Integration:** Diff endpoint reported +35 insertions, -2 deletions across `src/lib.rs` and `tests/parser_tests.rs`. Integrated into `main` at commit `fa8a4727f35d94b47a71c5adfcbfd323d98b2408`.

---

## 4. Key Failure Modes of Raw Agents vs. Axonel Advantages

| Dimension | Raw Agent (Baseline A) | Axonel Supervised (Baseline B) |
|---|---|---|
| **Non-Interactive Batch Mode** | Exits prematurely or halts on first friction; cannot self-supervise multi-step objectives. | Multi-cycle execution engine continuously drives progress until physical stopping conditions are satisfied. |
| **Crash & Interruption Resilience** | Ephemeral; any crash, terminal closure, or network hiccup loses all context and inflight work. | ACID SQLite persistence guarantees zero state loss; crashes resume from the last recorded cycle/checkpoint. |
| **Verification Reliability** | Relies on the agent self-reporting that its work succeeded (often hallucinated or false). | Independent physical verification executes out-of-band compiler commands (`cargo test`) directly on disk. |
| **Working Tree Safety** | Mutates the user's active workspace directly, risking dirty state and merge conflicts. | Isolates work in dedicated Git worktrees/workspaces; developer reviews diff before explicit integration. |
| **Developer Cognitive Load** | High ("babysitting tax"); developer must watch terminal output and manually verify files. | Zero; developer creates mission, continues other work, inspects diff via CLI/UI, and clicks/commands integrate. |

---

## 5. Conclusion & Product Milestone Sign-Off

The empirical data gathered in Milestone 17 definitively validates the Axonel product wedge:
1. **Background Supervision is Essential:** Standalone agents without supervisor loops fail in background developer workflows.
2. **Worktree Isolation + Independent Verification is the Minimum Viable Loop:** By isolating repository changes and independently running compilers, Axonel turns brittle AI generation into robust, verifiable software engineering.
3. **Architecture is Validated:** The frozen substrate (`plexis-core`, `plexis-storage`, `plexis-runtime`) and the new control-plane endpoints (`/diff`, `/integrate`, `/run`, and CLI commands) performed flawlessly under real workloads.
