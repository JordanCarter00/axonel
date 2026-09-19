# Changelog

All notable changes to **Axonel** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] - 2026-09-19

### Overview
Initial public release candidate of **Axonel**, a local-first autonomous engineering supervisor daemon designed to supervise external CLI coding agents (such as Google Gemini CLI) in isolated Git worktrees with out-of-band test verification and explicit human acceptance gates.

### Major Capabilities
- **Isolated Worktree Provisioning:** Executes agent tasks exclusively in isolated Git worktrees (`.plexis/worktrees/<task_id>`), leaving the active development tree and current Git branch untouched during background execution.
- **Independent Physical Verification:** Rejects LLM self-reports of success; runs out-of-band verification commands (`cargo test`, `npm test`, `pytest`) directly against the filesystem.
- **Clean Tree & Commit Invariants:** Enforces `working_tree_clean == true` and `required_commit_exists == true` before considering any task candidate.
- **Autonomous Multi-Cycle Replanning:** Detects stagnation and unmet stopping conditions, triggering bounded recovery cycles with adaptive prompt mutations.
- **Explicit Human Acceptance Gate:** Autonomous execution halts at `AwaitingAcceptance`. Direct unaccepted integration attempts return HTTP 409 Conflict.
- **Structured Review Packages (`GET /api/v1/missions/{id}/review`):** Serves unified deliverables, git diffs, changed files lists, verification receipts, and HEAD freshness indicators.
- **Transactional Git Integration & Crash Reconciliation:** Intermediate `Integrating` intent logging; startup reconciler queries Git ancestry (`git merge-base --is-ancestor`) to restore consistency after abrupt `SIGKILL` termination without false reporting.
- **Local Control Plane & Dashboard:** REST API and single-page Web Operations Dashboard served from loopback default.

### Supported Providers & Platforms
- **Supported Provider:** Google Gemini CLI v0.60.0 (`gemini_cli`) via official CLI subprocess adapter with streaming JSON and headless flags (`--approval-mode yolo --skip-trust`).
- **Internal Test Provider:** Deterministic local mock agent (`fake_agent`) for integration testing and automated regression suites.
- **Scaffold Stubs:** Anthropic Claude Code (`claude_code`), OpenAI Codex (`codex`), and local OpenCode (`opencode`) registered as interface stubs for post-v1 expansion.
- **Supported Platform:** Linux x86_64 (`x86_64-unknown-linux-gnu`).

### Security Model
- **Default Loopback Binding:** Server binds strictly to `127.0.0.1:3000` by default.
- **External Bind Authentication:** Binding to non-loopback addresses (`0.0.0.0` or public interfaces) requires `--auth-token` or `AXONEL_AUTH_TOKEN`; startup fails immediately with exit code 1 if unauthenticated.
- **Automated Secret Redaction:** In-memory redactor scrubs Bearer tokens, OpenAI/Anthropic API keys, Google API keys, and GitHub access tokens from terminal streaming buffers, logs, and events.
- **Workspace Path Confinement:** Resolves canonical paths and rejects directory traversal attempts (`..`) outside workspace boundaries.

### Known Limitations
- External Gemini CLI execution requires active local credentials (`gemini auth login` or `GEMINI_API_KEY`).
- Tasks are confined to single Git repositories; multi-repository coordination is out of scope for v0.1.0.
- Merges that encounter git merge conflicts against concurrent branch commits are aborted cleanly and surfaced to the operator; automatic hunk conflict resolution is not attempted.
- Prebuilt binaries are certified strictly for Linux x86_64; Linux aarch64 is planned but unverified on hardware.

### Compatibility & Migration Notes
- Initial public release; no prior stable API versions to migrate.
- Internal crate names (`plexis-core`, `plexis-runtime`, `plexis-storage`, `plexis-server`) are retained for internal stability; public CLI and product identity are unified under `axonel`.
