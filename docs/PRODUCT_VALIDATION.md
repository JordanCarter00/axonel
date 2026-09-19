# Plexis Product Validation Plan & Falsifiable Experiments

**Document Status:** Complete & Verified  
**Date:** September 19, 2026  
**Scope:** Definition of cheap, falsifiable experiments, concrete success/failure evidence criteria, and telemetry metrics to validate the Plexis product thesis without premature engineering investment.

---

## 1. Validation Strategy & Philosophy

Validating an autonomous developer tool is notoriously prone to confirmation bias. When creators test their own agents, they tend to guide prompts unconsciously, overlook manual interventions, or test against synthetic toys.

To prevent self-deception, this validation plan enforces four strict principles:
1. **No Fabricated Data:** We do not invent customer quotes, synthetic NPS scores, or pretend corporate pilots.
2. **Physical, Falsifiable Gates:** Success must be determined by physical machine artifacts: exit codes, compiler diagnostics, and Git commits, not qualitative self-reports from LLMs.
3. **Real Repositories Only:** All experiments must execute against real codebases (Rust, TypeScript, Python) with non-trivial dependencies and build systems.
4. **Walk-Away Criterion:** A test only succeeds if the human operator literally walked away from the machine during execution. Any human keystroke in the target worktree before the mission ends constitutes a failure of autonomy.

---

## 2. Falsifiable Product Experiments

---

### Experiment 1: Autonomous Flaky-Test & Concurrency Remediation (Technical Proof of Value)

#### Hypothesis
> An external agent supervised by Plexis can autonomously investigate a non-deterministic flaky test in an isolated Git worktree, identify the race condition, implement a thread-safe fix, verify that 50 consecutive runs pass on disk, and produce an atomic Git commit with zero human guidance.

#### Test Setup
- **Target Repository:** A real Rust multi-threaded repository (or a fork containing a genuine race condition, e.g., an uncoordinated atomic flag or channel timing race).
- **Execution:** Dispatch mission:
  ```bash
  plexis mission run "Isolate and fix the intermittent test failure in tests/concurrency.rs; verify with 50 consecutive test runs"
  ```
- **Supervisor Configuration:** Worktree isolation enabled; `GeminiCliBackend` with headless execution; `WorkspaceVerifier` configured to run `cargo test --test concurrency -- --nocapture` 50 times in a loop.

#### Success Evidence
1. The developer's primary working tree remains untouched throughout the run.
2. The agent executes within the worktree and modifies only the target concurrency code.
3. `WorkspaceVerifier` independently executes the 50-run test loop on disk and receives exit code 0 on all 50 iterations.
4. Mission completes with state `Completed`, producing a clean Git commit with a detailed commit message.
5. Total wall-clock time is under 30 minutes, and total token cost is under $1.50.

#### Failure Evidence
1. The agent loops more than 4 times without modifying files.
2. The agent claims the test is fixed, but the independent verifier reports a failure on iteration 12 of 50.
3. The mission exceeds budget caps ($3.00 or 45 minutes) and is killed by `BudgetTracker`.
4. Git merge conflicts or uncommitted file residue remains in the worktree.

#### What We Learn
- Does the supervisory loop (Agent -> Edit -> Out-of-band Verifier -> Recovery) genuinely eliminate the "hallucinated pass" failure mode that plagues naked CLI agents?

#### Next Decision
- **If Passed:** Flaky-test remediation is validated as our primary flagship demonstration for the initial public launch.
- **If Failed:** If failure is due to agent reasoning, investigate prompting/tooling improvements. If due to verification timeouts, refine verification batching.

---

### Experiment 2: Breaking Dependency Migration (Developer Workflow Trial)

#### Hypothesis
> Developers will choose to dispatch Plexis in the background for breaking API version bumps (e.g., upgrading a web framework from v0.6 to v0.7) rather than running interactive agents (Claude Code / Cursor) because Plexis allows them to continue their primary coding tasks without editor disruption.

#### Test Setup
- **Target Repository:** A TypeScript/Node.js or Rust web service repository requiring a major library upgrade (e.g., migrating from `axum 0.6` to `0.7` with breaking extractor signatures, or upgrading `@tanstack/react-query` v4 to v5).
- **Trial Process:**
  1. 5 experienced developers are given the task of upgrading the dependency.
  2. Developers run Plexis via CLI: `plexis mission run "Upgrade axum to 0.7 and fix all compiler and routing errors"`.
  3. Developers are instructed to continue their regular daily coding in their main VS Code / Neovim editor.
  4. Track developer context-switching, active screen time, and completion rates.

#### Success Evidence
1. Developers spend less than 3 minutes initiating and configuring the mission.
2. Developers spend zero time watching or waiting for the agent during the upgrade execution.
3. Plexis successfully creates a branch `plexis/mission-axum-upgrade` with passing build (`cargo check` = 0) and passing unit tests.
4. Developers merge the PR branch into `main` with minimal or no manual adjustments.

#### Failure Evidence
1. Developers report anxiety about what the agent is doing and continuously switch to the terminal or dashboard to inspect it.
2. Plexis corrupts dependencies, edits unrelated files, or hallucinates deprecation fixes that don't compile.
3. Developers state they would rather use Cursor/Composer interactively because they want immediate line-by-line control.

#### What We Learn
- Validates the core psychological premise of Plexis: does the developer actually *want* to walk away, or does the lack of synchronous interaction create distrust?

#### Next Decision
- **If Passed:** Build out dedicated CLI commands for dependency bumps (`plexis migrate <pkg> <version>`).
- **If Failed:** Enhance the Web UI real-time streaming timeline and push notifications to provide high-trust transparency during background execution.

---

### Experiment 3: Daemon Crash & Hard Reboot Durability (Operational Resilience Test)

#### Hypothesis
> The Plexis SQLite persistence layer, startup lease reconciler, and checkpoint engine can survive unexpected OS termination (`kill -9`, power loss, terminal crash) during active agent execution, automatically resuming the mission without human data loss or corrupted Git branches.

#### Test Setup
- **Target Repository:** The Plexis repository itself or a standard benchmark repo.
- **Execution:**
  1. Start a multi-cycle mission that takes ~15 minutes.
  2. At minute 7 (during active `GeminiCliBackend` subprocess execution), issue `pkill -9 -f plexis-server` to simulate an immediate process crash.
  3. Verify that the agent process group is cleaned up or orphaned.
  4. Restart `plexis-server`.
  5. Inspect startup logs, database state, and lease reconciliation.

#### Success Evidence
1. Upon restart, `Reconciler` detects the interrupted mission in state `Active`.
2. Orphaned leases are fenced or re-acquired monotonically.
3. The mission state is restored to the last valid immutable checkpoint (`MissionCheckpointRecord`).
4. The mission resumes execution from the interrupted cycle and proceeds to a passing verification.
5. The Git worktree is left in a consistent state without git lock-file errors (`index.lock`).

#### Failure Evidence
1. `plexis-server` crashes on startup due to database lock contention or poisoned state.
2. The mission becomes permanently stuck in `Active` or enters an unrecoverable `Failed` state.
3. Git reports `fatal: Unable to create '.git/index.lock': File exists`.
4. The resumed agent executes duplicate work, burning double tokens.

#### What We Learn
- Proves whether Plexis provides enterprise-grade durability that distinguishes it from fragile, ephemeral terminal scripts.

#### Next Decision
- **If Passed:** Freeze the storage and reconciliation architecture as battle-tested.
- **If Failed:** Fix lease fencing invariants and git lock cleanup in `plexis-runtime/src/reconciler.rs`.

---

### Experiment 4: Unsolvable Task Stagnation & Escalation Circuit Breaker

#### Hypothesis
> When presented with an impossible task (e.g., fixing a bug caused by a missing proprietary binary or contradictory trait requirements), Plexis will detect stagnation within 3 cycles, halt execution, preserve the worktree, and escalate cleanly to human approval rather than burning tokens in an infinite prompt loop.

#### Test Setup
- **Target Repository:** A Rust crate with an intentionally contradictory trait bound that cannot compile under Rust's type system, or a missing external system library (`libfoo.so`).
- **Mission:** `plexis mission run "Make cargo test pass"`.
- **Budget:** Max cycles = 5, Max tokens = 200,000, Max minutes = 15.

#### Success Evidence
1. `RecoveryController` observes repeated identical compiler errors across 2 cycles.
2. `LivenessEvaluator` marks the mission as `Stagnant`.
3. Mission transitions to `AwaitingApproval` or `Failed` with reason `StagnationDetected`.
4. Token expenditure stops immediately.
5. The mission dashboard displays a human-readable diagnosis explaining why the agent stalled, along with the compiler error trace.

#### Failure Evidence
1. The agent continues running until it exhausts the maximum budget cap or token limit.
2. The agent deletes the failing test suite to force `cargo test` to pass (cheating).
3. The daemon crashes or hangs waiting for agent output.

#### What We Learn
- Proves whether Plexis has solved the "infinite token burn" problem that terrifies developers using autonomous agents.

#### Next Decision
- **If Passed:** Include the Stagnation Breaker as a key safety feature in marketing and documentation.
- **If Failed:** Tighten heuristic diff-similarity and error-repetition thresholds in `LivenessEvaluator`.

---

## 3. Measurable Product Metrics & Telemetry Specification

To ensure ongoing evaluation remains empirical, the Plexis daemon will compute the following telemetry metrics per mission and across the global workspace:

### 1. Autonomous Completion Rate (ACR)
- **Formula:**
  $$\text{ACR} = \frac{M_{\text{verified}}}{M_{\text{total}}} \times 100$$
  *Where $M_{\text{verified}}$ is missions reaching `Completed` with passing out-of-band verification, and $M_{\text{total}}$ is all non-draft missions.*
- **MVP Target:** $\ge 75\%$ on targeted tech-debt & test-repair missions.

### 2. Verification Integrity Rate (VIR)
- **Formula:**
  $$\text{VIR} = \frac{M_{\text{true\_positive}}}{M_{\text{agent\_claimed\_done}}} \times 100$$
  *Measures how often an agent claimed it succeeded, but the physical disk verifier confirmed it was actually green.*
- **Target:** $100\%$ (Plexis must NEVER accept a hallucinated pass).

### 3. Human Intervention Rate (HIR)
- **Formula:**
  $$\text{HIR} = \frac{M_{\text{escalated}}}{M_{\text{completed}}} \times 100$$
  *Measures how many completed missions required human assistance via approval or replanning.*
- **MVP Target:** $\le 20\%$.

### 4. Mean Time to Verified Result (MTVR)
- **Formula:**
  $$\text{MTVR} = \frac{1}{N} \sum_{i=1}^{N} (T_{\text{complete}, i} - T_{\text{start}, i})$$
  *Elapsed wall-clock time from `plexis mission run` to verified Git commit.*
- **MVP Target:** $\le 20\text{ minutes}$.

### 5. Stagnation Catch Latency (SCL)
- **Formula:**
  $$\text{SCL} = \text{Average number of cycles elapsed before an unresolvable mission is halted}$$
- **MVP Target:** $\le 2.5\text{ cycles}$.

### 6. Cost Per Completed Mission (CPCM)
- **Formula:**
  $$\text{CPCM} = \frac{\sum \text{Model API Token Costs} + \text{Compute Costs}}{M_{\text{verified}}}$$
- **MVP Target:** $\le \$2.00$ per verified mission using Gemini 2.5 Pro / Flash.
