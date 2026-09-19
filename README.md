# Axonel

> **The Local-First Autonomous Engineering Supervisor Daemon**  
> *Dispatch multi-hour coding tasks in isolated Git worktrees, enforce hard multi-dimensional budgets, verify code independently on disk, and walk away with confidence.*

[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)
[![Architecture Freeze](https://img.shields.io/badge/architecture-frozen%20(M16)-success.svg)](docs/MILESTONE_16.md)
[![Substrate Tests](https://img.shields.io/badge/e2e%20verification-passing%20(M15)-brightgreen.svg)](docs/MILESTONE_15.md)

---

## What is Axonel?

**Axonel** is an autonomous software engineering operating system and supervisor daemon. Rather than acting as another interactive chat assistant or prompt wrapper, Axonel operates as a **local hypervisor for real external coding agents** (Google Gemini CLI, Anthropic Claude Code, etc.).

Axonel solves the **"babysitting tax"** on medium-to-long-horizon engineering tasks (30 minutes to 4 hours):
- **Never hijacks your editor:** Operates in dynamically provisioned, isolated Git worktrees (`axonel/mission-<id>`). You continue writing features in your primary editor without uncommitted changes being clobbered or branches switching.
- **Never accepts hallucinated passes:** Out-of-band supervisor verifiers independently execute compilers (`cargo check`, `tsc`) and test suites (`cargo test`, `npm test`) directly on disk. If an agent claims "everything passes" while compilation fails, Axonel rejects the cycle and triggers recovery.
- **Never burns infinite budgets:** Enforces strict, multi-dimensional circuit breakers (wall-clock timeouts, turn caps, token spend limits, and diff-similarity stagnation detectors).
- **Survives crashes and reboots:** Backed by ACID SQLite WAL persistence and monotonic lease fencing. If your laptop reboots or the daemon restarts, active missions are cleanly restored to their last immutable checkpoint.

```text
                  ┌──────────────────────────────────────────────┐
                  │ 1. DEVELOPER STATES OBJECTIVE                │
                  │    "Fix race condition in tests/concurrency" │
                  └──────────────────────┬───────────────────────┘
                                         │
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 2. REPOSITORY & BASELINE DISCOVERY           │
                  │    - Capture git HEAD & verify clean tree    │
                  │    - Run baseline test suite to isolate flake│
                  └──────────────────────┬───────────────────────┘
                                         │
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 3. PROVISION ISOLATED GIT WORKTREE           │
                  │    - Branch `axonel/mission-<id>`            │
                  │    - Developer continues coding in VS Code   │
                  └──────────────────────┬───────────────────────┘
                                         │
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 4. AUTONOMOUS MULTI-CYCLE AGENT SUPERVISION  │
                  │    - LocalAgentHost executes real CLI agent  │
                  │    - Enforce turn, token & wall-clock caps   │
                  └──────────────────────┬───────────────────────┘
                                         │
                         ┌───────────────┴───────────────┐
                         ▼                               ▼
                 [Agent Stalls/Loops]             [Agent Completes]
                         │                               │
                         ▼                               ▼
      ┌─────────────────────────────────────┐ ┌────────────────────────────────────┐
      │ 5. SUPERVISORY RECOVERY & REPLAN   │ │ 6. INDEPENDENT PHYSICAL VERIFICATION │
      │    - Detect stagnation / zero diff   │ │    - WorkspaceVerifier executes on disk│
      │    - Revert broken cycle checkpoint │ │    - Compiler exit code == 0          │
      │    - Mutate prompt / agent role     │ │    - Test suite pass count == 100%    │
      │    - Resume next execution cycle    │ │    - Git working tree clean & commited│
      └──────────────────┬──────────────────┘ └─────────────────┬──────────────────┘
                         │                                      │
                         └───────────────┬──────────────────────┘
                                         ▼
                  ┌──────────────────────────────────────────────┐
                  │ 7. VERIFIED DELIVERABLE & HUMAN REVIEW       │
                  │    - Structured review package (diff & logs) │
                  │    - Explicit human operator acceptance gate │
                  │    - Atomic merge into target branch         │
                  └──────────────────────────────────────────────┘
```

---

## Why Axonel? The Competitive Gap

Current coding agents fall into two extreme categories, leaving an unaddressed whitespace:

| Capability | Interactive CLIs (Claude Code, Aider) | In-IDE AI (Cursor, Windsurf) | Cloud Agents (Devin, Factory) | **Axonel Daemon** |
| :--- | :--- | :--- | :--- | :--- |
| **Execution Environment** | Local terminal process | Local IDE process | Ephemeral Cloud VM | **Local Supervisor Daemon** |
| **Isolation Primitive** | None (active working tree) | None (active editor tab) | Remote Debian VM | **Isolated Git Worktrees** |
| **Developer Governance** | In-loop prompt reviews | In-loop editor reviews | Cloud async PR review | **Supervised background execution with human review before merge** |
| **Verification Authority** | Agent self-report (LLM) | IDE LSP / diagnostics | In-VM test runner | **Out-of-band disk verifier** |
| **Crash Durability** | State lost on terminal exit | State lost on editor close | Cloud database | **ACID SQLite + Checkpoints** |
| **Control Plane & Privacy** | Local API calls | Local / Remote proxy | Code uploaded to cloud | **Local control plane; external LLM egress governed by chosen provider** |
| **Operating Cost** | API token pass-through | Monthly subscription ($20)| $500+/month seat license | **Free & Open Source** |

*For our complete 15-dimension competitive audit, see [`docs/MILESTONE_16_COMPETITIVE_ANALYSIS.md`](docs/MILESTONE_16_COMPETITIVE_ANALYSIS.md).*

---

## The Product Wedge: Background Tech-Debt & Flaky-Test Remediation

From our Milestone 16 strategic research, Axonel is deliberately focused on an acute, falsifiable product wedge:

> **Autonomous Background Tech-Debt, Breaking Dependency Upgrades & Flaky-Test Remediation**

### Why this wedge wins:
1. **Objective, Binary Verification:** Compilers (`rustc`, `tsc`) and test runners (`cargo test`, `pytest`) provide an unambiguous pass/fail boundary. Success does not rely on subjective design tastes.
2. **Eliminates High-Friction Tasks:** Developers universally dread mechanical refactors (e.g. migrating `axum 0.6` to `0.7`, bumping major ORM versions, or hunting down flaky async race conditions).
3. **Autonomous Background Execution:** You dispatch the mission via CLI or UI and continue your main work. Axonel executes and verifies in the background, surfacing a structured review package when stopping conditions pass on disk.

*Read the full Product Requirement Document in [`docs/PRODUCT.md`](docs/PRODUCT.md).*

---

## Architectural Principles & Frozen Core

Following Milestone 16, Axonel enforces a strict architectural boundary to prevent feature creep from destabilizing the core runtime:

```text
                     PRODUCT EXPERIENCE LAYER (ADAPTIVE)
          [Developer CLI (`axonel`)]   [Web Dashboard]   [PR / Git Exporter]
                                       │
                                       ▼
                       ─────────────────────────────────
                       FROZEN ARCHITECTURAL BOUNDARY (API)
                       ─────────────────────────────────
                                       │
                                       ▼
                             AXONEL CORE SUBSTRATE
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

### Core Substrate Invariants:
1. **Durable State is Authoritative:** Live processes and agent sessions are ephemeral. All missions, cycles, leases, and events are stored in transactional SQLite with WAL mode.
2. **Independent Verification:** Agent claims are untrusted. Verification is conducted by an out-of-band supervisor daemon inspecting disk exit codes and filesystem diffs.
3. **Monotonic Lease Fencing:** Multi-agent concurrent tasks are governed by atomic lease generations to prevent split-brain filesystem overwrites.
4. **Agent Agnosticism:** External agents are treated strictly as unprivileged OS subprocesses communicating via streaming JSON or standard CLI pipes.

---

## Repository Structure

The engine is organized as a unified Rust workspace and modern TypeScript frontend:

```text
axonel/
├── crates/
│   ├── plexis-core/         # Pure domain models, 11-state Mission FSM, TaskGraph DAG, typed IDs
│   ├── plexis-storage/      # Transactional SQLite engine, WAL mode, foreign keys, migrations 0001-0005
│   ├── plexis-runtime/      # MissionEngine, LocalAgentHost, GeminiCliBackend, WorkspaceVerifier, WorktreeManager
│   ├── plexis-tools/        # Built-in host tools, Bubblewrap sandbox, secret redactor, symlink guards
│   ├── plexis-planner/      # Autonomous DAG decomposition, heuristic proposal generator, plan validator
│   ├── plexis-memory/       # Hierarchical memory scopes, hybrid semantic vector + BM25 indexing
│   ├── plexis-providers/    # Direct LLM provider failover router (OpenAI, Gemini, Anthropic, Ollama)
│   └── plexis-server/       # Axum REST API server, SSE streaming, terminal ring buffers, static SPA host
├── web/                     # React 18 + TypeScript + Vite + Tailwind CSS Mission Operations Dashboard
├── docs/                    # Authoritative product specifications, competitive analysis, and milestone audits
│   ├── PRODUCT.md           # Product Thesis, Wedge Selection, Core Loop, and PRD
│   ├── MILESTONE_16_COMPETITIVE_ANALYSIS.md # Sourced 7-agent architectural breakdown
│   ├── PRODUCT_VALIDATION.md# Falsifiable validation experiments & telemetry metrics
│   ├── MILESTONE_16.md      # Milestone 16 synthesis & architecture freeze report
│   └── MILESTONE_15.md      # Milestone 15 long-horizon mission proof & verification audit
├── migrations/              # Authoritative SQL schema migrations (0001 through 0005)
└── tests/                   # End-to-end integration tests & adversarial stress suites
```

---

## Getting Started

### Prerequisites
- **Rust:** 1.80+ (2021 edition)
- **Node.js:** 18+ (for Web UI dashboard)
- **Git:** 2.30+ (with `git-worktree` support)
- **Google Gemini CLI:** (Optional, for real autonomous missions via `gemini-3.1-flash-lite`)

### 1. Build and Verify Workspace
```bash
# Check the entire workspace
cargo check --workspace --all-targets

# Run the complete unit and integration test suite
cargo test --workspace
```

### 2. Launch the Control Plane & Web UI
Start the Axonel daemon (REST API + static dashboard):
```bash
cargo run -p plexis-server
```

Or run the frontend development server with live reload:
```bash
cd web
npm install
npm run dev
```

Visit **`http://localhost:3000`** to view the Mission Operations Dashboard.

---

## REST API & Real-Time SSE Streaming

The control plane exposes structured HTTP endpoints and live Server-Sent Events (SSE):

| Endpoint | Method | Description |
| :--- | :--- | :--- |
| `/health` | `GET` | System health check and version reporting |
| `/api/v1/missions` | `GET` / `POST` | List or dispatch autonomous long-horizon missions |
| `/api/v1/missions/:id` | `GET` | Detailed mission state, budget consumption, and cycle history |
| `/api/v1/missions/:id/plan` | `POST` | Trigger autonomous task graph decomposition |
| `/api/v1/missions/:id/start` | `POST` | Start autonomous multi-cycle execution loop |
| `/api/v1/workflows` | `GET` / `POST` | Manage fine-grained TaskGraph DAG workflows |
| `/api/v1/approvals` | `GET` | List pending human-in-the-loop escalation gates |
| `/api/v1/approvals/:id/approve` | `POST` | Sign off on an escalation gate with operator notes |
| `/api/v1/workspaces/:id/git/diff` | `GET` | View live unified git diff generated in worktree |
| `/api/v1/events/stream` | `GET` | Real-time SSE stream with `Last-Event-ID` cursor replay |

---

## Milestone Evolution & Engineering History

Axonel has been constructed across 21 rigorous milestones:

- [x] **Milestone 1: Repository Foundation & Core Invariants** — Domain models, typed UUIDv7 IDs, SQLite WAL storage, monotonic leases.
- [x] **Milestone 2: Execution Plane & Sandboxing** — Host process runner, Bubblewrap namespaces, secret redaction.
- [x] **Milestone 3: Scheduler, Planner & Multi-Agent Engine** — Topological DAG scheduler, dynamic decomposition, strategy mutation.
- [x] **Milestone 4: Persistent Memory & Runtime Hardening** — 8 memory scopes, hybrid vector + BM25 keyword search.
- [x] **Milestone 5: Real-World Autonomous Workloads** — Autonomous repository modification, integration verification.
- [x] **Milestone 6: Production Readiness & State Transition Audit** — Fencing tokens, symlink defense, planner budgets.
- [x] **Milestone 8: Browser Verification & UX Audit** — Playwright E2E audit, live SSE proof, zero mocked state.
- [x] **Milestone 9: Developer-Grade Productization** — CLI `init`/`status`/`serve`, workspace management, diff viewer.
- [x] **Milestone 10: Real AI Workflow & Product Hardening** — Provider failover, 7 domain affinities, GitHub REST API, budget alerts.
- [x] **Milestone 11: Real Multi-Agent Worktree Isolation** — Concurrent agent execution across isolated Git worktrees.
- [x] **Milestone 12: Distributed Fencing & Durable Messaging** — Atomic lease fencing, inter-agent messaging channels.
- [x] **Milestone 13: Real Gemini CLI Supervision** — Spawning headless `gemini` CLI subprocesses with JSON streaming and YOLO approval.
- [x] **Milestone 14: Mission Substrate & Long-Horizon Recovery** — 11-state Mission FSM, multi-cycle replanning, SQLite checkpoints.
- [x] **Milestone 15: Autonomous Long-Horizon Coding Proof** — Autonomous mission execution: real Gemini CLI repaired a Rust repository, passed `cargo test` on disk, and created a verified Git commit with zero human edits.
- [x] **Milestone 16: Product Wedge & Architecture Freeze** — Comprehensive competitive audit across 7 agent architectures, PRD definition, validation plan, and frozen core boundary.
- [x] **Milestone 17: Gemini CLI Subprocess Hardening** — Robust process group (PGID) signals, headless streaming, timeout traps.
- [x] **Milestone 18: End-to-End Autonomous Coding Proof** — Defect injection, autonomous repair, disk test pass, candidate commit.
- [x] **Milestone 19: Human Governance & Release Boundary** — Explicit `AwaitingAcceptance` state, unified review packages, accept/reject decisions.
- [x] **Milestone 20: Transactional Integration & Reliability** — Durable `Integrating` state, crash recovery reconciliation, workspace locking, conflict rollback, stale-target guard.
- [x] **Milestone 21: Public Release Hardening** — Loopback-only security defaults, unified Git integration engine, honest provider status, CI automation, reproducible release.
- [x] **Milestone 22: Real-World Validation** — 20-task real-world engineering benchmark across Rust, TypeScript, and Python; direct Gemini baseline comparison; long-horizon multi-turn replanning; crash-recovery verification; operational responsibility audit.

---

## Comprehensive Documentation

For deep technical specifications, security threat models, and validation benchmarks, consult the [`docs/`](docs/) directory:
- 📊 **[Real-World Validation Results (`docs/REAL_WORLD_VALIDATION_RESULTS.md`)](docs/REAL_WORLD_VALIDATION_RESULTS.md)**: Empirical 20-task benchmark, baseline comparisons, operational value analysis, and failure taxonomy.
- 🛡️ **[Security Threat Model (`docs/SECURITY_MODEL.md`)](docs/SECURITY_MODEL.md)**: Trust boundaries, worktree confinement, shell risks, secret redaction, and known limitations.
- 📦 **[Reproducible Release Guide (`docs/REPRODUCIBLE_RELEASE.md`)](docs/REPRODUCIBLE_RELEASE.md)**: Step-by-step instructions to compile, verify, and package Axonel deterministically.
- ⚖️ **[Claims Audit & Truthfulness Ledger (`docs/CLAIMS_AUDIT.md`)](docs/CLAIMS_AUDIT.md)**: Formal classification of supported, partially supported, and retracted claims.
- 🧪 **[Real-World Validation Framework (`docs/REAL_WORLD_VALIDATION.md`)](docs/REAL_WORLD_VALIDATION.md)**: Structured dataset schema, evaluation protocol, and KPIs for agent benchmarking.
- 📄 **[Product Requirement Document (`docs/PRODUCT.md`)](docs/PRODUCT.md)**: Product thesis, target user personas, core loop, MVP boundary, and metrics.
- 🔬 **[Competitive Analysis (`docs/MILESTONE_16_COMPETITIVE_ANALYSIS.md`)](docs/MILESTONE_16_COMPETITIVE_ANALYSIS.md)**: Detailed architectural breakdown of Claude Code, Gemini CLI, OpenHands, Amux, Devin, Cursor, and Aider.
- 📐 **[Architecture Specification (`ARCHITECTURE.md`)](ARCHITECTURE.md)**: Complete system design, state machines, and concurrency invariants.

---

## License

Axonel is licensed under the Apache License, Version 2.0 or the MIT License, at your option.
