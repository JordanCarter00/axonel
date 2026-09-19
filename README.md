# Axonel

> **The Local-First Autonomous Engineering Supervisor Daemon**  
> *Dispatch multi-hour coding tasks in isolated Git worktrees, enforce hard multi-dimensional budgets, verify code independently on disk, and walk away with confidence.*

[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)
[![Release Candidate](https://img.shields.io/badge/release%20candidate-v0.1.0--rc1-brightgreen.svg)](docs/V1_RELEASE_CHECKLIST.md)
[![Architecture Freeze](https://img.shields.io/badge/architecture-frozen%20(M16)-success.svg)](docs/MILESTONE_16.md)
[![Substrate Tests](https://img.shields.io/badge/e2e%20browser-15%2F15%20passing-brightgreen.svg)](web/tests/e2e_release_candidate.mjs)

---

## What is Axonel?

**Axonel** is a local-first autonomous engineering supervisor daemon. Rather than acting as another interactive chat assistant or prompt wrapper, Axonel operates as a **local hypervisor for real external coding agents** (Google Gemini CLI, Anthropic Claude Code, etc.).

Axonel manages the complete engineering mission lifecycle:
$$\text{Objective} \to \text{Autonomous Mission} \to \text{Worktree Isolation} \to \text{External Process Supervision} \to \text{Out-of-Band Physical Verification} \to \mathbf{AwaitingAcceptance} \to \mathbf{Explicit Acceptance} \to \text{Safe Git Integration}$$

---

## What Problem Does It Solve?

Frontier coding models possess strong reasoning capabilities, but when deployed directly in interactive CLIs (Claude Code, Gemini CLI, Aider) or in-IDE chat (Cursor), they impose a heavy **"babysitting tax"**:
1. **Working Tree Hijacking:** Direct agents edit files directly in your active working directory. If you switch branches or edit code, your working tree is corrupted.
2. **Hallucinated Passes:** Agents frequently claim that tests pass when they never executed them or when compilation failed silently.
3. **Dirty Residue & Untracked Files:** Agents edit files and run build tools, but omit `git commit` or leave untracked build artifacts.
4. **Zero Crash Durability:** If your terminal closes, Wi-Fi drops, or the machine reboots, all active agent execution state is lost.

Axonel solves these failure modes by providing an **isolated, verifiable, crash-safe supervisor** so you can dispatch a background task and walk away.

---

## How is Axonel Different from Claude Code / Gemini CLI?

| Capability | Raw Interactive CLI (Gemini CLI / Claude Code) | Axonel Supervisor Daemon |
| :--- | :--- | :--- |
| **Working Directory** | Hijacks your active working tree; corrupts uncommitted work | Dynamically provisions isolated Git worktrees (`.plexis/worktrees/<id>`) |
| **Verification Authority** | Relies on LLM's self-report in terminal text | Out-of-band execution of `cargo test`, `npm test`, or `pytest` directly on disk |
| **Tree Hygiene Invariant** | Leaves uncommitted changes and untracked build artifacts | Enforces `working_tree_clean == true` and `required_commit_exists == true` |
| **Stagnation & Recovery** | Stalls indefinitely or terminates on first error | Autonomous multi-cycle replanning with prompt mutation and state rewind |
| **Human Governance** | Developer must manually review `git diff` in terminal | Structured Review Package with unified diff, changed files list, and verification receipt |
| **Branch Protection** | Agent can directly mutate your main branch | Autonomous missions halt at `AwaitingAcceptance`; unaccepted code is strictly blocked |
| **Crash Durability** | Process dies on terminal exit; all state is lost | ACID SQLite WAL persistence; crashes reconcile durably via Git ancestry checks |
| **Operating Cost** | API token pass-through | Free & Open Source local control plane |

---

## How Do I Install It?

### Prerequisites
- **Linux** (x86_64 or aarch64)
- **Rust / Cargo** $\ge$ 1.80 (`rustup update stable`)
- **Node.js** $\ge$ 18.0 & **npm**
- **Git** $\ge$ 2.34 (with `git worktree` support)
- **Google Gemini CLI** (optional, for live agent execution via `gemini`)

### Clean Linux Install from Source
```bash
# 1. Clone the repository
git clone https://github.com/axonel/axonel.git
cd axonel

# 2. Build the Web Dashboard assets
npm --prefix web ci
npm --prefix web run build

# 3. Compile the release binary
cargo build --release -p plexis-server --bin axonel

# 4. (Optional) Install to system PATH
sudo cp target/release/axonel /usr/local/bin/
```

---

## How Do I Run My First Mission?

### Step 1: Start the Axonel Daemon
By default, Axonel binds safely to loopback (`127.0.0.1:3000`):
```bash
axonel serve
```

### Step 2: Register a Repository Workspace
In a separate terminal, register the repository you want Axonel to work on:
```bash
# Register via CLI (or through the Web UI at http://127.0.0.1:3000)
curl -X POST http://127.0.0.1:3000/api/v1/workspaces \
  -H "Content-Type: application/json" \
  -d '{"name": "my-project", "canonical_path": "/absolute/path/to/my-project"}'
```

### Step 3: Dispatch an Autonomous Mission
```bash
curl -X POST http://127.0.0.1:3000/api/v1/missions \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Fix failing integration test",
    "objective": "Fix test_quoted_values_stripped in tests/integration_test.rs by stripping enclosing quotes in src/parser.rs. Commit changes with git.",
    "workspace_id": "<WORKSPACE_ID>",
    "stopping_condition": {
      "command": "cargo test",
      "working_tree_clean": true,
      "required_commit_exists": true
    },
    "auto_start": true
  }'
```

### Step 4: Review, Accept & Integrate
1. Open the Web Dashboard at **`http://127.0.0.1:3000`**.
2. When the mission reaches **`READY FOR REVIEW`** (`AwaitingAcceptance`), click **Review Package**.
3. Inspect the unified deliverable diff, changed files list, and out-of-band test receipts.
4. Click **Accept & Integrate** to merge the deliverable into your target branch.

---

## What Guarantees Does Axonel Provide?

1. **Zero Main-Branch Pollution:** Missions run exclusively in isolated Git worktrees. Your active editor and main branch are never touched during execution.
2. **Independent Physical Verification:** Tests and build checks execute directly on disk; agent self-reports are never trusted.
3. **Explicit Human Acceptance Gate:** Autonomous code is strictly blocked from merging until an operator explicitly accepts the deliverable.
4. **Crash-Safe Durability:** All mission state is backed by transactional SQLite WAL persistence. Daemon crashes or reboots recover without state loss or false reporting.
5. **Strict Budget Circuit Breakers:** Bounded wall-clock timeouts, turn limits, and stagnation detectors prevent infinite loops and runaway costs.
6. **Security by Default:** Binds to `127.0.0.1` by default; requires an explicit `--auth-token` to bind to non-loopback interfaces.

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

---

## Empirical Release Evidence

Axonel v0.1.0-rc1 is certified against genuine, machine-readable evidence:

| Evidence Dimension | Verified Result | Verification Source |
| :--- | :--- | :--- |
| **Real Browser E2E** | **15 / 15 PASSED (100%)** | `web/tests/e2e_release_candidate.mjs` via Playwright Chromium |
| **Security Defaults Suite** | **6 / 6 PASSED (100%)** | `crates/plexis-server/tests/security_tests.rs` |
| **Real Gemini CLI Execution** | **PASS (Proven)** | Google Gemini CLI repaired code, passed `cargo test` on disk, committed cleanly |
| **Crash & Recovery Durability** | **100% Reconciled** | SIGKILL recovery verified via Git ancestry check without data loss |
| **Human Acceptance Gate** | **100% Enforced** | Unaccepted integration returns HTTP 409 Conflict; review package verified |
| **Multi-Language Tasks** | **Recorded in Telemetry** | `docs/validation/results.json` tracks exact observed trials across Rust, TS, and Python |
| **Remote CI Status** | **100% Green** | GitHub Actions `.github/workflows/ci.yml` passing on remote `main` |
| **Compiler Quality Gate** | **0 Warnings / 0 Errors** | `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` |

*For the complete 18-point checklist, see [`docs/V1_RELEASE_CHECKLIST.md`](docs/V1_RELEASE_CHECKLIST.md).*

---

## Authoritative Documentation

- 📋 **[v1 Release Checklist (`docs/V1_RELEASE_CHECKLIST.md`)](docs/V1_RELEASE_CHECKLIST.md)**: Formal 18-point release candidate verification with explicit PASS/FAIL status.
- 📊 **[Real-World Validation Results (`docs/REAL_WORLD_VALIDATION_RESULTS.md`)](docs/REAL_WORLD_VALIDATION_RESULTS.md)**: Empirical baseline comparison, operational value analysis, and failure taxonomy.
- 🧪 **[Validation Dataset & Schema (`docs/REAL_WORLD_VALIDATION.md`)](docs/REAL_WORLD_VALIDATION.md)**: 20-task benchmark corpus schema and evaluation protocol.
- 🛡️ **[Security Threat Model (`docs/SECURITY_MODEL.md`)](docs/SECURITY_MODEL.md)**: Trust boundaries, worktree confinement, shell risks, secret redaction, and known limitations.
- 📦 **[Reproducible Release Guide (`docs/REPRODUCIBLE_RELEASE.md`)](docs/REPRODUCIBLE_RELEASE.md)**: Step-by-step instructions to compile, verify, and package Axonel deterministically.
- 📄 **[Product Requirement Document (`docs/PRODUCT.md`)](docs/PRODUCT.md)**: Product thesis, target user personas, core loop, and MVP boundary.
- 🔬 **[Competitive Analysis (`docs/MILESTONE_16_COMPETITIVE_ANALYSIS.md`)](docs/MILESTONE_16_COMPETITIVE_ANALYSIS.md)**: Detailed architectural breakdown of Claude Code, Gemini CLI, OpenHands, Amux, Devin, Cursor, and Aider.

---

## License

Axonel is licensed under the Apache License, Version 2.0 or the MIT License, at your option.
