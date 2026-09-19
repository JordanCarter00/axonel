# Milestone 16 — Product Wedge, Competitive Revalidation & Architecture Freeze

**Milestone Status:** Complete  
**Date:** September 19, 2026  
**Scope:** Research, Competitive Audit, Codebase Asset Mapping, Architecture Freeze, and Product Requirements for the Plexis Platform.

---

## 1. Executive Summary & Context

Milestone 15 brought the Plexis technical engine to a major milestone of capability:
- Durable long-horizon `Mission` state machines with 11 states and multi-cycle execution.
- Real Google Gemini CLI process execution in headless streaming JSON mode with full autonomous file edits and Git commits.
- Independent physical disk verification (`WorkspaceVerifier`) confirming compiler exit codes (`cargo test = 0`) and clean working trees.
- Isolated Git worktree execution, inter-agent messaging, budget enforcement, recovery replanning, startup reconciliation, and React 18 Web UI visualization.

Following the completion and verification of Milestone 15, **Milestone 16 was instituted as a deliberate pause on infrastructure development**. Rather than adding another generic orchestration subsystem or provider, Milestone 16 answers the fundamental question:

> **What specific painful problem should Plexis solve that existing coding-agent products and agent orchestration systems do not already solve sufficiently?**

Milestone 16 delivers four comprehensive artifacts:
1. `docs/MILESTONE_16_COMPETITIVE_ANALYSIS.md` — Sourced architectural breakdown across 7 agent archetypes over 15 dimensions.
2. `docs/PRODUCT.md` — Product thesis, target user personas, wedge selection, core loop, architecture freeze, and MVP scope.
3. `docs/PRODUCT_VALIDATION.md` — Concrete, falsifiable validation experiments, success/failure criteria, and telemetry metrics.
4. `docs/MILESTONE_16.md` — This milestone synthesis, asset mapping, architecture review, and scope constraints.

---

## 2. Research Performed & Competitive Findings

We conducted a technical audit of the agentic coding landscape across seven major systems:
- **Claude Code (Anthropic):** The premier terminal REPL, but tightly bound to an active interactive terminal session. Imposes a severe **babysitting tax** on tasks lasting longer than 10 minutes. Lacks durable background execution, worktree isolation, and independent verification.
- **Gemini CLI (Google):** Exceptional raw throughput and 2M token context, but purely an un-orchestrated execution endpoint without multi-cycle state, worktrees, or supervisor-level budget circuit breakers.
- **OpenHands (All-Hands AI):** Pioneers open-source Docker-sandboxed agents. However, heavy Docker containers create extreme local development friction (slow startup, high RAM usage, broken local toolchain caching), and its linear EventStream cannot handle concurrent worktree DAGs.
- **Amux / Dmux / Herdr:** Validates that **Git worktrees are the superior local isolation primitive** for coding agents. However, they are "dumb" multiplexers without supervisory intelligence, lease fencing, or automated verification.
- **Devin (Cognition AI):** The benchmark for autonomous background task resolution. However, its 100% hosted cloud VM architecture introduces enterprise IP barriers, high cost ($500+/month), and disconnection from local development environments.
- **Cursor (Anysphere):** Dominates synchronous in-editor autocomplete and diff editing, but fundamentally unsuited for background "fire-and-forget" tasks.
- **Aider (Paul Gauthier):** Established Git-commit discipline and test feedback loops, but architecturally limited to a single-agent terminal session.

### The Unmet Architectural Whitespace
Existing tools force developers to choose between **interactive terminal/IDE babysitting** (Claude Code, Cursor, Aider) or **expensive, closed-source cloud VMs** (Devin).

There is no mature, local-first tool that allows a developer to dispatch a complex, 1-hour engineering task in the background, continue their own coding in their primary editor uninterrupted, and trust that a supervisor daemon will isolate the work in a Git worktree, enforce budget caps, independently verify the results on disk, and recover from crashes.

---

## 3. Existing Plexis Asset Map

We performed an audit of the entire Plexis codebase across all crates and packages:

```
================================================================================
                           PLEXIS ASSET QUADRANT MAP
================================================================================

┌──────────────────────────────────────────┬──────────────────────────────────────────┐
│ 1. REUSABLE CORE (FROZEN SUBSTRATE)      │ 2. PRODUCT-SPECIFIC (ADAPTIVE LAYER)     │
├──────────────────────────────────────────┼──────────────────────────────────────────┤
│ • plexis-core/src/mission.rs             │ • plexis-server/src/routes.rs            │
│   (11-state Mission FSM, cycle records)  │   (HTTP endpoints, SSE event streams)    │
│ • plexis-core/src/workflow.rs            │ • web/src/components/MissionsView.tsx    │
│   (Workflow & TaskGraph DAG validation)  │   (React 18 dashboard, cycle timeline)   │
│ • plexis-core/src/lease.rs               │ • plexis-planner/src/planner.rs          │
│   (Monotonic lease fencing, tokens)      │   (AutonomousDecomposer heuristic rules) │
│ • plexis-storage/src/sqlite/             │ • web/src/components/DiffViewer.tsx      │
│   (ACID SQLite, migrations 0001-0005)    │   (In-browser git diff inspector)        │
│ • plexis-runtime/src/agent_host/         │                                          │
│   (LocalAgentHost, PGID process control) │                                          │
│ • plexis-runtime/src/backend/gemini_cli.rs│                                         │
│   (Headless stream-json, YOLO execution) │                                          │
│ • plexis-runtime/src/verifier.rs         │                                          │
│   (WorkspaceVerifier out-of-band checks) │                                          │
│ • plexis-runtime/src/worktree.rs         │                                          │
│   (WorktreeManager git isolation)        │                                          │
│ • plexis-runtime/src/governance.rs       │                                          │
│   (BudgetTracker, token/turn/time caps)  │                                          │
│ • plexis-runtime/src/recovery.rs         │                                          │
│   (RecoveryController, replan mutations) │                                          │
│ • plexis-runtime/src/reconciler.rs       │                                          │
│   (Startup crash & lease reconciler)     │                                          │
├──────────────────────────────────────────┼──────────────────────────────────────────┤
│ 3. POTENTIALLY UNNECESSARY / DORMANT     │ 4. MISSING (MVP PRD REQUIREMENTS)        │
├──────────────────────────────────────────┼──────────────────────────────────────────┤
│ • plexis-memory/                         │ • Dedicated developer CLI (`plexis`)     │
│   (Vector embeddings, semantic search.   │   (`plexis run`, `status`, `merge`)      │
│    Not needed for compiler-driven wedge) │ • Worktree merge automation              │
│ • plexis-providers/                      │   (Clean integration back into main)     │
│   (Multi-provider HTTP retry/failover.   │ • Automatic test-failure reproducer      │
│    Superseded by CLI process wrappers)   │   (Looping failing tests before/after)   │
│ • plexis-fake-agent/                     │ • Worktree disk cache sharing            │
│   (Test mocks, strictly test-only)       │   (Preventing duplicated target/ caches) │
└──────────────────────────────────────────┴──────────────────────────────────────────┘
```

---

## 4. Product Wedge Decision

We evaluated three potential product wedges:
1. **Wedge 1: The Autonomous Background Tech-Debt & Flaky-Test Remediation Daemon**
2. **Wedge 2: "Fire-and-Forget" Asynchronous Feature Builder**
3. **Wedge 3: Pre-PR Verification & Agent Hypervisor for CI/CD**

### Selection: Wedge 1 (Tech-Debt & Flaky-Test Daemon)
- **Target User:** Solo developers, tech leads, and small engineering teams maintaining Rust, TypeScript, or Python repositories.
- **The Pain:** Mechanical refactors, breaking dependency upgrades, and flaky tests consume 20–30% of engineering time. Developers cannot afford to babysit interactive agents for hours through repetitive compiler fixes.
- **Why Plexis Wins:**
  - Plexis runs in the background on an isolated Git worktree.
  - The developer's primary editor is never hijacked or corrupted.
  - Plexis enforces independent physical verification (`cargo test = 0`) on disk, eliminating hallucinated completions.
  - Built-in crash recovery guarantees that laptop reboots or network drops do not lose work.

---

## 5. Architectural Freeze: Boundary Definition

To prevent future product development from compromising the core execution engine, we draw a strict, frozen architectural boundary:

```text
                           PRODUCT EXPERIENCE LAYER
             [Developer CLI]   [Web Dashboard]   [PR / Git Exporter]
                                       │
                                       ▼
                       ─────────────────────────────────
                       FROZEN ARCHITECTURAL BOUNDARY (API)
                       ─────────────────────────────────
                                       │
                                       ▼
                             PLEXIS CORE SUBSTRATE
          [plexis-core]      [plexis-storage]     [plexis-runtime]
          - Mission FSM      - SQLite Store       - LocalAgentHost (PGID)
          - TaskGraph DAG    - Migrations 0001-05 - WorktreeManager
          - Leases & Tokens  - Checkpoint Store   - WorkspaceVerifier
          - Protocol Envelop - Audit Event Log    - BudgetTracker & Recovery
                                       │
                                       ▼
                            EXTERNAL AGENT ADAPTERS
                        [GeminiCliBackend]  [ClaudeCodeBackend]
```

### Invariants of the Frozen Core:
1. **Core Invariance:** No modifications to `plexis-core` state machines, transition rules, or storage schemas without proof of an invariant bug.
2. **Strict Unidirectional Dependency:** The Product Layer may call into the Core Substrate. The Core Substrate must **never** import or depend on the Product Layer (CLI UI, React components, GitHub REST APIs).
3. **Agent Agnosticism:** External agents are treated strictly as unprivileged OS subprocesses conforming to standard JSON streams or CLI arguments.

---

## 6. Architecture Review: Foundational Gaps & Remediation

We audited the existing implementation for foundational issues that could obstruct productization:

### Must Fix Before MVP
1. **Missing Developer CLI Interface:**
   - *Issue:* Currently, missions must be triggered via `curl` against `plexis-server` or via the web browser. Developers demand a terminal-native tool.
   - *Fix:* Implement a lightweight, ergonomic `plexis` CLI crate providing `plexis init`, `plexis run "<objective>"`, `plexis status`, `plexis diff`, and `plexis merge`.
2. **Worktree Artifact & Cache Duplication:**
   - *Issue:* In Rust repositories, each new Git worktree creates its own `target/` directory by default, which can consume 5–10 GB of disk space and trigger redundant re-compilations.
   - *Fix:* Configure `WorktreeManager` to inject `CARGO_TARGET_DIR` pointing to a shared workspace target directory with proper file locking.
3. **Worktree Directory Garbage Collection on Crash:**
   - *Issue:* If the machine crashes (`SIGKILL`), the SQLite database reconciles the lease on startup, but the physical worktree directory on disk may remain orphaned in `.git/worktrees`.
   - *Fix:* Enhance `Reconciler` to cross-reference filesystem worktrees against active database leases and prune abandoned worktrees.
4. **Flaky-Test Multi-Run Assertion:**
   - *Issue:* A single passing run of a flaky test does not prove the flake is resolved.
   - *Fix:* Allow `WorkspaceVerifier` to accept an iteration count parameter (e.g., `consecutive_runs: 20`) before declaring verification success.

### Can Remain Later (Post-MVP)
1. **OS Containerization (cgroups / Landlock):** Running CLI agents directly on the host using `LocalAgentHost` process groups is sufficient for local developer machines where the developer already trusts the CLI tool. Full sandboxing can be added in a later security milestone.
2. **Remote Cloud GitHub App Integration:** Webhook-based PR creation can wait until the local CLI experience is completely polished.
3. **Distributed Multi-Host Execution:** Unnecessary for the single-workstation developer wedge.

---

## 7. What Should NOT Be Built Yet (Scope Constraints)

To resist feature creep and maintain sharp product focus, the following capabilities are explicitly declared **OUT OF SCOPE** for the immediate development cycle:

- ❌ **NO Custom Code Editor:** Do not build an IDE, text editor, or VS Code fork.
- ❌ **NO Cloud Hosting / SaaS Backend:** Do not build user authentication, cloud billing, or remote worker clusters.
- ❌ **NO Proprietary Foundation Model Training:** Do not train local weights. Supervise frontier models.
- ❌ **NO Multi-Language Plugin Ecosystem:** Do not invent a custom WASM plugin architecture. Use standard Unix shell scripts and CLI binaries.
- ❌ **NO Broad Greenfield Feature Prototyping:** Do not attempt open-ended "build me an entire e-commerce store" features. Focus exclusively on deterministic, test-verifiable engineering missions.

---

## 8. Sourced References & Citations

1. Anthropic, "Claude Code Research Preview & Tool Documentation", `https://docs.anthropic.com/en/docs/agents-and-tools/claude-code`, February 2025.
2. Google Cloud, "Gemini CLI and Vertex AI Generative AI Documentation", `https://github.com/GoogleCloudPlatform/gemini-cli`, 2024–2025.
3. Wang et al., "OpenHands: An Open Platform for AI Software Developers as Generalist Agents", arXiv:2407.16741, July 2024.
4. Cognition AI, "Devin Technical Report & SWE-bench Performance Analysis", `https://cognition.ai/blog/swe-bench-technical-report`, 2024.
5. Paul Gauthier, "Aider: AI Pair Programming in Your Terminal", `https://aider.chat`, 2023–2025.
6. Coder, "Amux: Agent Multiplexer for Tmux and Git Worktrees", `https://github.com/coder/amux`, 2025.
7. Git SCM, "Git Worktree Documentation (`git-worktree`)", `https://git-scm.com/docs/git-worktree`.
8. Jimenez et al., "SWE-bench: Can Language Models Resolve Real-World GitHub Issues?", ICLR 2024, arXiv:2310.06770.
