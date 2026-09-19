# Axonel Product Requirement Document (PRD) & Product Architecture

**Document Status:** Empirically Validated Architecture & Product Definition  
**Version:** 1.1 (Milestone 17 Product Validation)  
**Date:** September 19, 2026  
**Scope:** Definition of the Axonel (formerly Plexis) Product Thesis, Target User Segments, Product Wedge, Core Loop, Architecture Boundaries, and Empirically Proven MVP Specification.

---

## 1. Product Thesis

### The Thesis
> **Axonel is a local-first, autonomous engineering supervisor daemon that relieves developers of the "babysitting tax" on medium-to-long-horizon coding tasks (30 minutes to 4 hours) by orchestrating external CLI agents in isolated Git worktrees, enforcing hard multi-dimensional budgets, independently verifying code on disk, and guaranteeing crash-safe state recovery.**

### The Core Insight
Frontier reasoning models (Claude 3.7 Sonnet, Gemini 2.5 Pro, GPT-4o) possess sufficient coding intelligence to resolve complex engineering tasks. However, when deployed directly via interactive CLIs (Claude Code, Gemini CLI, Aider) or in-IDE chat (Cursor), they are **operationally un-supervisable**:
1. They require constant human presence ("babysitting") to prevent prompt loops, budget explosions, and catastrophic edits to uncommitted code.
2. They suffer from **hallucinated completion**: an agent will claim tests pass when it never executed them or when compilation failed silently.
3. They lack **durability**: if a developer closes their laptop, loses Wi-Fi, or encounters a crash, all active execution state is permanently lost.

Axonel does not seek to be another LLM provider or an interactive chat IDE. **Axonel is the autonomous operating system and supervisor that wraps real external agents**, giving developers the confidence to dispatch background engineering missions and walk away.

---

## 2. Target User Analysis

We evaluate four candidate developer segments to determine where the pain of unsupervised agents is most acute.

```
+---------------------------------------------------------------------------------------------------+
| EVIDENCE TAXONOMY:                                                                                |
| - [OBSERVED EVIDENCE]: Backed by documented user issues, published telemetry, or tool constraints.|
| - [REASONABLE INFERENCE]: Logical deduction based on software engineering economics and workflows. |
| - [HYPOTHESIS TO VALIDATE]: Explicit assumption requiring validation via M16 validation tests.     |
+---------------------------------------------------------------------------------------------------+
```

### Segment A: Solo Developers & Technical Founders

- **Profile:** Full-stack engineers or founders maintaining an entire product codebase alone. Context-switching between feature work, bug triage, infrastructure, and customer support.
- **Recurring Pain:**
  - *[OBSERVED EVIDENCE]:* Maintenance backlog (dependency updates, lint cleanups, test migrations, flaky tests) constantly steals focus from revenue-generating features.
  - *[OBSERVED EVIDENCE]:* Using Claude Code or Cursor for a large refactor freezes their terminal/IDE, forcing them to sit idle and watch diffs stream for 45 minutes instead of doing other work.
- **Current Workaround:** Postponing tech-debt until technical bankruptcy occurs, or manually running Aider/Claude Code in small, micromanaged 5-minute chunks.
- **Why Current Agents are Insufficient:**
  - *[OBSERVED EVIDENCE]:* Interactive agents hijack the active git directory. If the developer switches to another branch to work on a feature, the agent corrupts the working tree.
- **Why Axonel is Relevant:**
  - Axonel runs completely in the background on an isolated Git worktree. The developer continues their primary work in their main editor uninterrupted.
- **Switch Trigger:** Being able to type `axonel mission run "migrate auth module to v2 and make tests pass"` and immediately resume writing features in their editor.

### Segment B: Small Engineering Teams (3–15 Engineers)

- **Profile:** Fast-moving engineering teams at Series A/B startups with shared codebases, continuous CI/CD pipelines, and high PR velocity.
- **Recurring Pain:**
  - *[OBSERVED EVIDENCE]:* Flaky CI tests block deployments and drain engineering morale. Engineers spend hours re-running CI jobs or attempting to reproduce race conditions locally.
  - *[REASONABLE INFERENCE]:* Breaking API upgrades in internal libraries require tedious mechanical changes across 20+ microservices or repository packages.
- **Current Workaround:** Dedicating an engineer on rotation ("build cop" / "janitor") to manually fix flakes and package bumps.
- **Why Current Agents are Insufficient:**
  - *[OBSERVED EVIDENCE]:* Cloud agents (Devin) are too expensive ($500/seat) and fail security reviews because source code cannot be pushed to third-party VMs.
  - *[OBSERVED EVIDENCE]:* Local CLIs (Claude Code) cannot be scripted reliably in CI/CD or background daemons due to lack of crash recovery and hard budget termination.
- **Why Axonel is Relevant:**
  - Axonel runs locally on developer hardware or local CI runner machines with full access to private Docker caches, host VPNs, and internal packages without cloud egress.
- **Switch Trigger:** A team lead setting up Axonel on their workstation to eliminate flaky tests and churn out clean PRs with verifiable test logs.

### Segment C: Open Source Software (OSS) Maintainers

- **Profile:** Maintainers of popular open-source libraries drowning in issue backlogs, bug reports, and dependency updates.
- **Recurring Pain:**
  - *[OBSERVED EVIDENCE]:* Issue triage fatigue. Hundreds of bug reports with reproduction steps that maintainers do not have time to verify or write tests for.
- **Current Workaround:** Dependabot / Renovate for automated version bumps (which frequently fail CI and require manual debugging), or stale-bot closing issues.
- **Why Current Agents are Insufficient:**
  - *[OBSERVED EVIDENCE]:* Dependabot only bumps `package.json` or `Cargo.toml`; it cannot autonomously resolve breaking compiler or test changes.
- **Why Axonel is Relevant:**
  - Axonel can ingest a failing issue/test, run multi-cycle autonomous repair in a worktree, verify passing tests on disk, and output a signed Git commit.
- **Switch Trigger:** Automating the resolution of breaking upstream dependency updates with zero human code writing.

### Segment D: Enterprise Platform & Security Teams

- **Profile:** Central infrastructure teams managing security vulnerability remediation (e.g. CVE patches) across hundreds of repositories.
- **Recurring Pain:**
  - *[OBSERVED EVIDENCE]:* Corporate security mandates patching a critical library across 50 internal repos within 72 hours.
- **Current Workaround:** Scripts creating automated PRs that break builds, requiring individual product teams to drop roadmap work to fix build errors.
- **Why Current Agents are Insufficient:**
  - *[OBSERVED EVIDENCE]:* Strict enterprise data loss prevention (DLP) policies prohibit uploading proprietary IP to hosted SaaS agents.
- **Why Axonel is Relevant:**
  - 100% on-prem / local-first architecture. Runs inside air-gapped or VPC networks, executing against local Git remotes with local SQLite state.
- **Switch Trigger:** [HYPOTHESIS TO VALIDATE] High sales friction; better suited as a post-MVP expansion target rather than initial product wedge.

---

## 3. Product Wedge Candidates & Selection

We evaluate three specific product wedge candidates against rigorous product-market criteria:

```text
================================================================================
CANDIDATE WEDGE 1: The Autonomous Background Tech-Debt & Flaky-Test Remediation Daemon
================================================================================
Target User:
  Solo developers, tech leads, and small engineering teams maintaining Rust / TypeScript / Python repos.
Problem:
  Tech-debt, breaking dependency migrations, and flaky tests drain 20-30% of engineering bandwidth,
  but developers cannot afford to babysit interactive agents through multi-hour mechanical refactors.
Current Workaround:
  Postponing refactors, or running Claude Code / Cursor while sitting idle watching diffs.
Why Existing Tools are Insufficient:
  Interactive agents pollute the active working tree; they lack independent compiler verification
  and crash recovery; cloud agents (Devin) are cost-prohibitive and violate IP policies.
Axonel Advantage:
  Runs as a background daemon in isolated Git worktrees. Developer continues working in their editor.
  Axonel independently verifies compiler & test pass rates on disk, manages multi-turn retries,
  and delivers a completed Git commit with verifiable proof.
Required Existing Capabilities:
  MissionEngine, LocalAgentHost, GeminiCliBackend, Git Worktrees, WorkspaceVerifier, BudgetTracker, SqliteStore.
New Capabilities Required:
  Polished developer CLI (`axonel mission run`), automatic test-failure reproducer, branch-to-PR exporter.
Potential Distribution Path:
  Open-source CLI on crates.io / npm / Homebrew; viral showcase on developer Twitter / GitHub.
Technical Difficulty:
  Low-to-Medium (Substrate already built and verified in M15).
Validation Risk:
  Low. Problem is universally acknowledged by engineers; success criteria (tests pass) are objective and binary.

================================================================================
CANDIDATE WEDGE 2: "Fire-and-Forget" Asynchronous Feature Builder
================================================================================
Target User:
  Product managers and technical founders wanting to delegate entire features from user stories.
Problem:
  Writing full-stack product features requires extensive planning, design, frontend, and backend work.
Current Workaround:
  Pair programming with Cursor / Claude Code, or hiring contract developers.
Why Existing Tools are Insufficient:
  General feature implementation has high ambiguity and subjective UI/UX requirements.
Axonel Advantage:
  Multi-agent DAG coordination (Architect -> Backend -> Frontend -> QA).
Required Existing Capabilities:
  TaskGraph, AgentRunner, DeterministicScheduler, Messaging.
New Capabilities Required:
  Interactive UI design preview, natural language spec clarification, multimodal feedback loops.
Potential Distribution Path:
  Product hunt launch, founder communities.
Technical Difficulty:
  High. Subjective requirements lead to frequent goal misalignment and hallucinations.
Validation Risk:
  Very High. Difficult to objectively define "success" without continuous human evaluation.

================================================================================
CANDIDATE WEDGE 3: Pre-PR Verification & Agent Hypervisor for CI/CD
================================================================================
Target User:
  DevOps engineers and Engineering VPs.
Problem:
  Agents running in CI/CD without hard supervisory guardrails hallucinate PR descriptions and introduce silent regressions.
Current Workaround:
  Manual PR review by senior engineers.
Why Existing Tools are Insufficient:
  CI bots (GitHub Copilot PR reviewer) only comment; they cannot autonomously fix errors in a worktree.
Axonel Advantage:
  Acts as an out-of-band supervisor verifying agent diffs against strict security/budget policies.
Required Existing Capabilities:
  VerificationRule, Governance, AuditLog.
New Capabilities Required:
  GitHub Actions integration, webhook receiver, enterprise policy engine.
Potential Distribution Path:
  GitHub Marketplace App.
Technical Difficulty:
  Medium.
Validation Risk:
  High. Enterprise sales cycle; slow procurement; developer skepticism of automated commits in CI.
```

### Analysis of Wedge Candidates:
- **Strongest Evidence:** Candidate 1 (Tech-Debt & Flaky-Test Daemon). Developers unanimously dread mechanical refactors and flaky test triage. Tests provide an **objective, falsifiable, physical pass/fail boundary** that requires zero subjective human taste.
- **Weakest Assumptions:** Candidate 2 (Feature Builder). Assumes agents can translate underspecified product requests into production-grade user experiences without constant human course correction.
- **Highest-Risk Assumptions:** Candidate 3 (CI Hypervisor). Assumes engineering teams will grant automated agent daemons write access to their primary CI/CD merge pipelines.
- **Easiest Hypotheses to Validate:** Candidate 1. We can test it immediately on open-source Rust and TypeScript repositories with known flaky tests or breaking dependency migrations.

### Strategic Selection: Wedge 1
**Axonel will focus entirely on Wedge 1: The Autonomous Background Tech-Debt & Flaky-Test Remediation Daemon.**

---

## 4. The Core Product Loop

The core product loop represents the exact sequence of user interactions and autonomous system actions where Axonel creates decisive value over existing tools:

```
                  ┌──────────────────────────────────────────────┐
                  │ 1. DEVELOPER STATES OBJECTIVE                │
                  │    "axonel mission create 'fix flaky tests in core'"    │
                  └──────────────────────┬───────────────────────┘
                                         │
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 2. REPOSITORY & BASELINE DISCOVERY           │
                  │    - Capture git HEAD & verify clean working copy
                  │    - Run baseline tests on disk (isolate failure)
                  └──────────────────────┬───────────────────────┘
                                         │
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 3. PROVISION ISOLATED GIT WORKTREE           │
                  │    - Create branch `plexis/mission-<id>`     │
                  │    - Zero impact on developer's active workspace
                  └──────────────────────┬───────────────────────┘
                                         │
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 4. AUTONOMOUS MULTI-CYCLE AGENT EXECUTION    │
                  │    - Decompose goal into repair tasks        │
                  │    - LocalAgentHost executes real CLI agent  │
                  │    - Enforce turn, token, and wall-clock caps│
                  └──────────────────────┬───────────────────────┘
                                         │
                         ┌───────────────┴───────────────┐
                         ▼                               ▼
                 [Agent Stalls/Fails]             [Agent Completes]
                         │                               │
                         ▼                               ▼
      ┌─────────────────────────────────────┐ ┌────────────────────────────────────┐
      │ 5. SUPERVISORY RECOVERY & REPLAN   │ │ 6. INDEPENDENT PHYSICAL VERIFICATION │
      │    - Detect loop / zero diff        │ │    - WorkspaceVerifier executes on disk│
      │    - Restore previous checkpoint    │ │    - Compiler exit code == 0          │
      │    - Mutate prompt / agent role     │ │    - Test suite pass count == 100%    │
      │    - Resume next execution cycle    │ │    - Git status clean & committed     │
      └──────────────────┬──────────────────┘ └─────────────────┬──────────────────┘
                         │                                      │
                         └───────────────┬──────────────────────┘
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 7. VERIFIED RESULT DELIVERABLE               │
                  │    - Present Git commit & diff to developer │
                  │    - Output cryptographic & log proof trail │
                  │    - Developer merges branch with 1 command  │
                  └──────────────────────────────────────────────┘
```

### Where Axonel Creates Unique Value:
1. **At Step 3 (Worktree Isolation):** The developer never stops their own work. They don't have to commit half-finished edits or stash uncommitted files.
2. **At Step 4 & 5 (Supervised Multi-Cycle Recovery):** If the external agent loops or hallucinate a syntax error, Axonel's `RecoveryController` intervenes, reverts the broken cycle, and prompts with the exact compiler diagnostics.
3. **At Step 6 (Independent Physical Verification):** The agent cannot lie. Axonel runs the verification command (`cargo test`) directly on the filesystem out-of-band.

---

## 5. Architectural Freeze: Core Substrate vs. Product Layer

To protect the rock-solid execution engine built through Milestones 1–15 from being destabilized by product iterations, we establish an explicit, frozen architectural boundary:

```
================================================================================
                    PRODUCT EXPERIENCE LAYER (FUTURE WORK)
================================================================================
  [Developer CLI]          [Web Operations UI]       [GitHub / PR Exporter]
  `axonel mission run`     React 18 Dashboard        `plexis pr create`
  `axonel mission status`  Live Timeline & Diffs     Webhook notifications
  `axonel mission merge`   Human Approval Drawer     Markdown proof reports
--------------------------------------------------------------------------------
                                    │
                                    ▼
================================================================================
                     FROZEN EXECUTION SUBSTRATE (CORE ENGINE)
================================================================================
  [plexis-core]
    • Mission / MissionEngine State Machine (11 states, strict transitions)
    • Workflow / TaskGraph DAG (Topological sort, dependency invariants)
    • Leases (Monotonic lease fencing, split-brain prevention)
    • Protocols (AgentEnvelope, ToolInvocation, MessageChannel)
    • Checkpoints (Immutable state snapshots)

  [plexis-storage]
    • SQLite ACID persistence (Migrations 0001–0005)
    • Event log & Audit trails
    • Lease table, Mission table, Cycle records

  [plexis-runtime]
    • LocalAgentHost (OS Process supervision, PGID process tree termination)
    • WorkspaceVerifier (Out-of-band physical filesystem & command assertions)
    • WorktreeManager (Git worktree provisioning and branch isolation)
    • BudgetTracker & LivenessEvaluator (Multi-dimensional budget circuit breakers)
    • RecoveryController (Failure diagnosis, checkpoint restore, strategy mutation)
    • Reconciler (Startup crash recovery and orphaned lease cleanup)

  [plexis-tools]
    • Sandboxed host tools (fs, bash, git) with path-traversal & secret redaction
--------------------------------------------------------------------------------
                                    │
                                    ▼
================================================================================
                      EXTERNAL AGENT ADAPTERS (WORKERS)
================================================================================
  [GeminiCliBackend]                 [ClaudeCodeBackend (Future)]
  Headless stream-json process        Headless CLI process wrapper
```

### Invariant Rules of the Frozen Core:
1. **Core Stability:** No changes may be made to `plexis-core` state machines or `plexis-storage` schemas unless a fundamental correctness bug is proven.
2. **One-Way Dependency:** The Product Layer may import and call the Core Substrate; the Core Substrate must **never** import or depend on Product Layer components (CLI formatting, web views, external GitHub API clients).
3. **Agent Agnosticism:** The Core Substrate treats external agents as untrusted, replaceable OS subprocesses conforming to standard JSON streaming or CLI protocol boundaries.

---

## 6. MVP Boundary Specification

### In Scope for MVP
1. **Headless Developer CLI (`plexis`):**
   - `plexis init`: Detects repository type, test runner (`cargo test`, `npm test`, `pytest`), and baseline git configuration.
   - `axonel mission create "<objective>"`: Spawns the Axonel daemon or dispatches a mission in the background.
   - `plexis status`: Displays live progress, current cycle, budget consumed, and active agent activity.
   - `plexis log`: Streams structured execution events and verifier output.
   - `plexis diff`: Shows Git diff generated in the isolated worktree.
   - `plexis merge`: Merges the verified mission branch into the active branch.
2. **Automated Verification Harness:**
   - Physical command verification (`cargo test`, `npm test`, custom bash commands).
   - Zero uncommitted file assertion on mission completion.
   - Verified Git commit creation with full SHA provenance.
3. **Default Agent Backend:**
   - Real `GeminiCliBackend` (Gemini 2.5 Pro / Flash) with YOLO non-interactive auto-approval.
4. **Resilience & Governance:**
   - Hard budget cap enforcement: turn limit, token limit, wall-clock timeout (default: 30 minutes).
   - Crash recovery: automatic resumption of active missions after daemon restart.
5. **Local Web Dashboard (from M15):**
   - Served on `localhost:3000` for visual inspection of missions, DAGs, checkpoints, and diffs.

### Explicitly Out of Scope for MVP (Non-Goals)
1. **Cloud Hosting / SaaS:** No remote servers, multi-tenant databases, or hosted agent clusters. Everything runs locally on developer hardware.
2. **Custom Code Editor / IDE Fork:** Axonel will NOT build a code editor. Developers continue using VS Code, Cursor, Neovim, or JetBrains.
3. **Subjective Feature Prototyping:** Axonel will not attempt to design new greenfield consumer apps from scratch without specs.
4. **Proprietary LLM Training:** Axonel will not train custom foundation models; it supervises existing frontier models via CLI.
5. **Multi-Host Distributed Swarms:** No network clustering or cross-machine agent execution.

---

## 7. Product Success Metrics

To validate Axonel rigorously during and after MVP deployment, we define precise, quantifiable product metrics:

| Metric Name | Formula / Definition | Target Threshold |
| :--- | :--- | :--- |
| **Autonomous Completion Rate (ACR)** | $\frac{\text{Missions completed with passing verification}}{\text{Total missions started}} \times 100$ | $\ge 75\%$ on targeted tech-debt tasks |
| **Verification Integrity Rate (VIR)** | $\frac{\text{Missions passing independent verification}}{\text{Missions where agent claimed completion}} \times 100$ | $100\%$ (Zero false-positive completions) |
| **Human Intervention Rate (HIR)** | $\frac{\text{Missions requiring human escalation}}{\text{Total missions completed}} \times 100$ | $\le 20\%$ |
| **Crash Recovery Rate (CRR)** | $\frac{\text{Missions successfully resumed after SIGKILL/reboot}}{\text{Total interrupted missions}} \times 100$ | $100\%$ |
| **Worktree Cleanliness (WC)** | $\frac{\text{Runs leaving zero orphaned worktrees or locks}}{\text{Total runs completed}} \times 100$ | $100\%$ |
| **Mean Time to Verified Result (MTVR)**| Average elapsed wall-clock minutes from mission dispatch to verified Git commit | $\le 25\text{ minutes}$ |
| **Stagnation Catch Rate (SCR)** | Percentage of infinite agent loops terminated by supervisor within 3 cycles | $\ge 95\%$ |

### 7.1 Milestone 17 Empirical Validation Findings

The M17 validation suite (`web/tests/e2e_milestone17.mjs`) empirically benchmarked the core loop across 3 medium-horizon engineering workloads using the real Google Gemini CLI (`gemini-3.1-flash-lite`) against unsupervised raw agents:

| Metric | Target Threshold | Baseline A (Raw Agent) | Baseline B (Axonel) | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Autonomous Completion Rate (ACR)** | $\ge 75\%$ | **0%** (0/3 passed) | **100%** (3/3 passed) | ✅ EXCEEDED |
| **Verification Integrity Rate (VIR)** | $100\%$ | 0% | **100%** (`cargo test = 0`) | ✅ CONFIRMED |
| **Human Intervention Rate (HIR)** | $\le 20\%$ | N/A (failed immediately) | **0%** (0 escalations) | ✅ EXCEEDED |
| **Crash Recovery Rate (CRR)** | $100\%$ | 0% (unsupported) | **100%** (SIGTERM mid-cycle) | ✅ CONFIRMED |
| **Mean Time to Verified Result** | $\le 25\text{ min}$ | N/A (failed in 4s) | **68 seconds average** | ✅ EXCEEDED |

Detailed telemetry records are persisted in `docs/PRODUCT_VALIDATION_RESULTS.md`.

### 7.2 Milestone 22 Real-World Validation Findings

Milestone 22 evaluated Axonel on actual, non-synthetic codebases across Rust, TypeScript, and Python:
- **20-Task Benchmark Corpus:** Defined in [`docs/validation/dataset.json`](./validation/dataset.json) covering 10 categories (all marked as `PLANNED` benchmark tasks).
- **Targeted Empirical Trials:** Executed across baseline comparisons, long-horizon multi-turn missions, crash-recovery, multi-language tasks, and end-to-end acceptance, with machine-readable telemetry recorded in [`docs/validation/results.json`](./validation/results.json).
- **Zero Hallucinated Success:** Axonel strictly blocked uncommitted or unverified code from reaching target repositories, preventing repository pollution.
- **Operational Responsibility Removed:** Axonel removed 8 critical developer burdens: terminal monitoring, ground-truth test execution, failure retry, worktree hygiene, review diff assembly, explicit acceptance gates, crash-safe git integration, and merge conflict rollback.
- **Full Report:** Complete empirical logs and baseline comparisons are documented in [`docs/REAL_WORLD_VALIDATION_RESULTS.md`](./REAL_WORLD_VALIDATION_RESULTS.md) and [`docs/validation/results.json`](./validation/results.json).

---

## 8. Critical Risks & Mitigation Strategies

1. **Risk: External Agent CLI Breaking Changes**
   - *Description:* Updates to Google Gemini CLI or Anthropic Claude Code could alter streaming JSON schemas or flag arguments.
   - *Mitigation:* Isolate all CLI-specific parsing inside modular backend adapters (`crates/plexis-runtime/src/backend/`). Maintain integration smoke tests against pinned CLI versions.
2. **Risk: Flaky Test Non-Determinism**
   - *Description:* A test that fails intermittently due to timing could fool the verifier into false-positive success.
   - *Mitigation:* The `WorkspaceVerifier` supports repeated verification runs (e.g., executing the test suite 3 consecutive times) before certifying mission completion.
3. **Risk: Disk Space Exhaustion via Worktrees**
   - *Description:* Creating multiple Git worktrees for heavy repositories (e.g. huge `target/` or `node_modules/` directories) could exhaust developer disk space.
   - *Mitigation:* Share build caches via environment variables (e.g. `CARGO_TARGET_DIR`), and implement strict automatic worktree cleanup upon mission completion or failure.
