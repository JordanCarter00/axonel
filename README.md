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

**Axonel** is a local-first autonomous engineering supervisor daemon. Rather than acting as another interactive chat assistant or prompt wrapper, Axonel operates as a **local hypervisor for real external coding agents** (such as Google Gemini CLI).

Axonel manages the complete engineering mission lifecycle:
$$\text{Objective} \to \text{Autonomous Mission} \to \text{Worktree Isolation} \to \text{External Process Supervision} \to \text{Out-of-Band Physical Verification} \to \mathbf{AwaitingAcceptance} \to \mathbf{Explicit Acceptance} \to \text{Safe Git Integration}$$

---

## Why Would I Use It?

Frontier coding models possess strong reasoning capabilities, but when deployed directly in interactive CLIs (Claude Code, Gemini CLI, Aider) or in-IDE chat (Cursor), they impose a heavy **"babysitting tax"**:
1. **Working Tree Hijacking:** Direct agents edit files directly in your active working directory. If you switch branches or edit code, your working tree is corrupted.
2. **Hallucinated Passes:** Agents frequently claim that tests pass when they never executed them or when compilation failed silently.
3. **Dirty Residue & Untracked Files:** Agents edit files and run build tools, but omit `git commit` or leave untracked build artifacts.
4. **Zero Crash Durability:** If your terminal closes, Wi-Fi drops, or the machine reboots, all active agent execution state is lost.

Axonel removes this burden by running agents in isolated Git worktrees with out-of-band test verification and explicit human acceptance gates so you can dispatch background tasks and walk away.

---

## What Does Axonel Do Today?

- **Provisions isolated Git worktrees** (`.plexis/worktrees/<id>`) so your active working tree and main branch remain untouched.
- **Supervises external agent processes** under strict resource, turn, and wall-clock budgets.
- **Independently verifies code on disk** using authoritative toolchains (`cargo test`, `npm test`, `pytest`).
- **Enforces tree hygiene invariants:** strictly blocks uncommitted changes and dirty working trees from reaching your repository.
- **Halts at an explicit review gate (`AwaitingAcceptance`):** generates unified diffs, changed files lists, and test receipts.
- **Performs crash-safe Git integration:** reconciles durably on startup via Git ancestry checks (`git merge-base --is-ancestor`) and rolls back cleanly on merge conflicts.

---

## What Agent / Provider Works?

- **Google Gemini CLI v0.60.0 (`gemini_cli`):** **Fully Proven Tier 1 Production Provider.** Headless CLI execution with streaming JSON, multi-turn tool calling, and automated recovery. Requires valid local credentials (`gemini auth login` or `GEMINI_API_KEY`).
- **Fake Agent Mock (`fake_agent`):** Fully proven deterministic local mock for testing and CI regression suites.
- **Scaffold Stubs:** Anthropic Claude Code (`claude_code`), OpenAI Codex (`codex`), and local OpenCode (`opencode`) are registered as interface stubs for post-v1 expansion.

---

## What Does Independent Verification Mean?

Axonel **never trusts an LLM's self-report** of success. When an external agent finishes editing code:
1. The Axonel supervisor daemon executes your ground-truth test command (`cargo test`, `npm test`, `pytest`) out-of-band directly on the filesystem.
2. The daemon checks `working_tree_clean == true` to ensure zero uncommitted edits or untracked build residue.
3. The daemon checks `required_commit_exists == true` to verify that a genuine Git commit object exists in the repository.
4. Only if all stopping conditions pass does the mission advance to `AwaitingAcceptance`. If verification fails or stagnation is detected, Axonel initiates autonomous replanning cycles.

---

## What Are the Security Assumptions?

- **Safe Local Defaults:** Axonel binds strictly to `127.0.0.1:3000` (loopback) by default.
- **External Bind Rejection:** Binding to non-loopback addresses (`0.0.0.0` or public interfaces) requires an explicit `--auth-token` or `AXONEL_AUTH_TOKEN`; unauthenticated external startup is rejected with exit code 1.
- **100% Local Control Plane:** All SQLite mission state, worktrees, and audit events remain on local disk. No telemetry or source code is uploaded to third-party clouds by Axonel.
- **External Model Egress:** When using the Gemini CLI provider, prompt context is sent to Google's API by the Gemini CLI process under your Google Cloud credentials.
- **Automated Secret Redaction:** In-memory redactor scrubs Bearer tokens, OpenAI/Anthropic keys, Google API keys, and GitHub access tokens from streaming buffers and logs.

---

## How Do I Install It?

### Prerequisites
- **Linux x86_64** (`x86_64-unknown-linux-gnu`; aarch64 planned/unverified)
- **Rust / Cargo** $\ge$ 1.80 (`rustup update stable`)
- **Node.js** $\ge$ 18.0 & **npm**
- **Git** $\ge$ 2.34 (with `git worktree` support)
- **Google Gemini CLI** (optional, for live autonomous agent execution via `gemini`)

### Clean Linux Install from Source
```bash
# 1. Clone the repository
git clone https://github.com/axonel/axonel.git
cd axonel

# 2. Build the Web Dashboard assets
npm --prefix web ci
npm --prefix web run build

# 3. Compile the production release binary
cargo build --release -p plexis-server --bin axonel

# 4. (Optional) Install to system PATH
sudo cp target/release/axonel /usr/local/bin/
```

---

## How Do I Run My First Mission?

### Step 1: Start the Axonel Daemon
```bash
axonel serve # Binds safely to 127.0.0.1:3000
```

### Step 2: Register a Repository Workspace
In a separate terminal (or through the Web UI at `http://127.0.0.1:3000`):
```bash
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
