# Axonel Public v0.1.x Operational Limitations & Boundaries

**Version:** 0.1.1  
**Target:** Axonel v0.1.1 Public Release  
**Status:** Authoritative Limitation Disclosure  
**Date:** September 19, 2026  

This document provides a transparent, comprehensive accounting of Axonel's operational assumptions, supported integrations, known failure modes, and boundaries for version 0.1.1.

---

## 1. Proven vs. Scaffolded Agent Provider Integrations

| Backend Identifier | Provider / Tool | Implementation Status | Support Tier | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **`gemini_cli`** | Google Gemini CLI v0.60.0 | **Fully Proven End-to-End** | **Tier 1 (Production)** | Complete lifecycle verified: worktree creation, headless execution, out-of-band verification, review package, human acceptance, and Git integration. Requires valid credentials (`gemini auth login` or `GEMINI_API_KEY`). |
| **`fake_agent`** | Deterministic Local Test Mock | **Fully Proven** | **Tier 1 (Test / CI)** | Internal test harness backend used for deterministic regression suites and CI verification. Built on-demand if missing. |
| **`claude_code`** | Anthropic Claude Code CLI | **Scaffold Stub** | **Tier 3 (Stub / Experimental)** | Interface stub registered in registry; execution pipeline under development for post-v1. |
| **`codex`** | OpenAI Codex / Aider | **Scaffold Stub** | **Tier 3 (Stub / Experimental)** | Interface stub registered; not certified for live production execution in v0.1.1. |
| **`opencode`** | Local OpenCode Models | **Scaffold Stub** | **Tier 3 (Stub / Experimental)** | Interface stub registered; requires local Ollama/vLLM server tooling. |

> [!WARNING]
> Do NOT attempt to run production coding missions with `claude_code`, `codex`, or `opencode` in v0.1.1. Google Gemini CLI (`gemini_cli`) is currently the **only** external agent backend certified for autonomous execution in this release.

---

## 2. Supported Operating Systems & Hardware Architectures

- **Certified Supported:** **Linux x86_64** (`x86_64-unknown-linux-gnu`). Fully build-tested, quality-gated, and verified in continuous integration.
- **Minimum Supported Rust Version (MSRV):** **Rust 1.88.0+** (Rust 2024 edition compatibility).
- **Planned / Unverified:** **Linux aarch64** (`aarch64-unknown-linux-gnu`). Compatible in architecture, but not natively compiled and verified on actual ARM64 hardware in this release.
- **Unsupported:** **Windows** (all versions). Axonel relies strictly on POSIX process tree isolation (`killpg`, `setpgid`, `SIGTERM`/`SIGKILL` cascades) and Unix domain sockets.
- **Experimental:** **macOS**. Worktrees and SQLite operate cleanly; Bubblewrap sandboxing is disabled on Darwin.

---

## 3. Network Egress & LLM Privacy Model

- **Control Plane Privacy (100% Local):** The Axonel supervisor daemon, REST API, SQLite database (`plexis.db`), Git worktrees, unified diffs, and verification receipts remain strictly on local disk. No telemetry, code, or logs are uploaded to Axonel or any third-party SaaS cloud.
- **External LLM Egress:** When using the Google Gemini CLI backend, prompts, file context, and terminal output are transmitted by the Gemini CLI process to Google's API endpoints in accordance with your Google Cloud / Gemini authentication terms.
- **Redaction Defense:** Axonel runs an automated in-memory regex redactor (`SecretRedactor`) over all terminal streaming buffers, logs, and events, scrubbing Bearer tokens, OpenAI/Anthropic keys (`sk-...`), Google API keys (`AIza...`), and GitHub access tokens (`ghp_...`).

---

## 4. Verification & Stopping Condition Boundaries

- **Authoritative Disk Authority:** Verification is strictly physical (`cargo test`, `npm test`, `pytest`, `working_tree_clean == true`, `required_commit_exists == true`).
- **No Static Correctness Guarantee:** Axonel guarantees that *your test suite passed cleanly on disk*. It does not mathematically prove program correctness beyond what your test suite covers. If your test suite is weak, flaky, or missing assertions, passing verification does not prevent behavioral defects.
- **Flaky Test Sensitivity:** Non-deterministic tests with network or timing race conditions can cause false-negative verification rejections or trigger unnecessary replanning cycles.

---

## 5. Repository & Workspace Constraints

- **Single-Repository Scope:** Each mission is strictly confined to a single Git repository workspace. Multi-repository transactions (e.g. coordinated edits across microservices) are unsupported in v0.1.x.
- **Git Worktree Requirement:** The target repository must be a valid Git repository with `git worktree` support (`git >= 2.34`).
- **Strict Isolation & No Direct Main Commits:** Agent execution occurs strictly in isolated worktrees (`.plexis/worktrees/<task_id>`). Agents are strictly forbidden from committing directly to the target repository's `main` branch or staging database/lockfiles indiscriminately. Changes only reach `main` via transactional integration following explicit human acceptance.
- **Build Cache Disk Footprint:** Each concurrent mission provisions a dedicated worktree on disk. For heavy codebases with large build directories (`target/`, `node_modules/`), running multiple simultaneous missions can consume gigabytes of disk space. Ensuring `.gitignore` includes build directories is essential.
- **Merge Conflicts:** If the target branch has moved concurrently and a merge conflict arises during integration, Axonel aborts the merge cleanly (`git merge --abort`) and returns HTTP 409 Conflict. Axonel does not automatically resolve conflicting merge hunks.

---

## 6. Multi-Language Empirical Boundaries

In Milestone 22 and 23, empirical validation was conducted across Rust, TypeScript, and Python:
- **Rust:** Full compiler diagnostic parsing (`rustc`), `cargo check`, `cargo test`, and `cargo clippy` integration.
- **TypeScript / Node:** Verified with native Node test runner (`node --test`) and npm scripts (`npm test`).
- **Python:** Verified with `pytest` in virtual environments.
- **Other Languages (Go, Java, C++, Ruby):** Axonel supports arbitrary custom test commands (e.g. `go test ./...`), but specialized error scrapers for non-Rust compilers are not yet included.

---

## 7. Performance & Concurrency Limits

- **Single-Daemon Concurrency:** Recommended maximum of 2–4 concurrent active missions on typical developer workstations to avoid CPU and disk I/O saturation during concurrent compilation.
- **No Distributed Cluster Execution:** Axonel v0.1.0 runs strictly as a single-node daemon on a local machine. It does not distribute worktrees or tasks across Kubernetes or remote server clusters.

---

## 8. User Experience & Interface Boundaries

- **Local Web Operations Dashboard:** The dashboard at `http://127.0.0.1:3000` is designed for local developer inspection, reviewing unified diffs, tracking cycle recovery, and single-click acceptance.
- **No In-Editor Inline Autocompletion:** Axonel is an asynchronous background delegation supervisor, not a real-time keystroke copilot inside VS Code.
- **Terminal Streaming Latency:** Terminal logs from subprocesses are flushed to SSE streams in 100ms chunks; brief buffering delays may occur during heavy compiler bursts.
