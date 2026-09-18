# Milestone 13 — Real Gemini CLI Agent Adapter: Final Report & Architectural Audit

## Executive Summary

Milestone 11 proved that Plexis could autonomously execute an end-to-end software engineering objective (`objective → planner → DAG → specialized agents → tools → approval → recovery → verification → Git commit`) through its internal runtime loop.

Milestone 12 solved the physical OS process-boundary problem: introducing the `LocalAgentHost` and `AgentBackend` abstractions, process-group sandboxing (`setpgid(0, 0)`), environment scrubbing, stream translation, and Git provenance tracking using `plexis-fake-agent`.

Milestone 13 answers the definitive architectural question:
> **Can Plexis physically control and govern a real, commercial autonomous coding agent?**

This milestone integrates **Google Gemini CLI (`gemini` v0.60.0)** as Plexis's first production-grade external coding agent adapter. The implementation preserves complete architectural parity with the Milestone 12 process model without altering Plexis core foundations or introducing synthetic mock shortcuts into production paths.

---

## 1. Why Gemini CLI Was Chosen as the First Real Adapter

Plexis chose Google Gemini CLI as its inaugural real external coding agent adapter for several strategic and technical reasons:

1. **Native Headless & Non-Interactive Capabilities:** Gemini CLI provides explicit command-line flags (`-p/--prompt`, `-o/--output-format stream-json`, `--approval-mode`, `-y/--yolo`, `--skip-trust`) designed specifically for headless automation and non-interactive scripting.
2. **Structured Machine-Readable Output:** Unlike coding tools that only render terminal ANSI escape sequences or interactive TUI widgets, Gemini CLI emits a structured, line-delimited JSON stream (`stream-json`) documenting tool actions, file modifications, command executions, and status transitions.
3. **Transparent Local Authentication Model:** Gemini CLI stores its credentials in well-defined local configuration paths (`~/.gemini/google_accounts.json` for OAuth, or standard environment variables `GEMINI_API_KEY`, `GOOGLE_API_KEY`, and Application Default Credentials). This allows Plexis to inspect authentication state statically without spawning interactive login prompts.
4. **Permissive Automation Governance:** Gemini CLI supports fine-grained approval flags (`--approval-mode default|auto_edit|yolo`), mapping directly to Plexis's governance and execution policies (`autonomous`, `supervised`, `dry_run`, `audit_only`).
5. **Local Installation Presence:** Gemini CLI is installed locally on the host environment (`/home/roonakyadav/.local/bin/gemini`), providing an immediate, verifiable binary for capability discovery, version introspection, and process execution.

---

## 2. Target Architecture

The complete physical execution topology connects the browser UI down to the local Git repository through the hardened process host:

```text
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                 PLEXIS WEB UI                                    │
│   (DAG View • Task Detail Drawer • Live Terminal • Provenance Badges • Providers)│
└─────────────────────────────────────────┬────────────────────────────────────────┘
                                          │ HTTP / SSE / REST (/api/v1/*)
                                          ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                PLEXIS SERVER                                     │
│     (Axum Router • Live Capability Probes • Bearer Auth • DB State Machine)      │
└───────────────────┬───────────────────────────────────────────────┬──────────────┘
                    │                                               │
                    ▼                                               ▼
┌─────────────────────────────────────────┐       ┌────────────────────────────────┐
│             AGENT RUNNER                │       │       BACKEND REGISTRY         │
│  (Lease Fencing • Verifier • Recovery)  │       │ (GeminiCliBackend, FakeAgent)  │
└───────────────────┬─────────────────────┘       └────────────────┬───────────────┘
                    │                                               │
                    └───────────────────────┬───────────────────────┘
                                            ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                               LOCAL AGENT HOST                                   │
│  (Process Group Supervision • Timeout Guard • Environment Scrubber • Workspace)  │
└───────────────────────────────────────────┬──────────────────────────────────────┘
                                            │ Physical OS Process Boundary
                                            │ tokio::process::Command (PGID == PID)
                                            │ Direct CLI Args: -p ... -o stream-json
                                            ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                      REAL GEMINI CLI PROCESS (gemini v0.60.0)                    │
│               [Dedicated Process Group PGID == PID; cmd.process_group(0)]        │
│                                                                                  │
│   1. Parse Objective & Workspace Context (-p "..." --skip-trust)                 │
│   2. Inspect Workspace Filesystem (Cargo.toml, src/lib.rs)                       │
│   3. Apply File Mutations & Run Tests (cargo test)                               │
│   4. Stream Structured Events to stdout (-o stream-json)                         │
│   5. Complete or Report Diagnostics                                              │
└───────────────────┬───────────────────────────────────────────────┬──────────────┘
                    │                                               │
     stdout Stream  ▼                                               ▼ Physical Changes
┌─────────────────────────────────────────┐       ┌────────────────────────────────┐
│          GEMINI STREAM PARSER           │       │    TARGET WORKLOAD REPO        │
│    (Translates stream-json events to    │       │  (Independent Git Verifier:    │
│       Plexis ExecutionEvent NDJSON)     │       │   diff, log, test inspection)  │
└─────────────────────────────────────────┘       └────────────────────────────────┘
```

---

## 3. `GeminiCliBackend` Implementation Details

The `GeminiCliBackend` (`crates/plexis-runtime/src/backend/gemini/backend.rs`) implements the core `AgentBackend` trait:

```rust
pub struct GeminiCliBackend {
    host: Arc<LocalAgentHost>,
    probe: Arc<GeminiCapabilityProbe>,
    configured_model: Option<String>,
}
```

Key operational characteristics:
* **Identification:** `id()` returns `"gemini_cli"`, and `display_name()` returns `"Google Gemini CLI"`.
* **Dynamic Availability Check:** `is_available()` evaluates `self.probe.probe().is_usable()` in real time. It confirms both that the executable is present on disk and that valid credentials exist.
* **Pre-flight Credential Gate:** In `execute()`, the backend runs a pre-flight probe. If the CLI is installed but lacks active credentials, execution fails fast with an explicit diagnostic (`RuntimeError::ExternalAgent("authentication_required: ...")`) instead of launching an orphaned process or hanging on interactive browser prompts.
* **Process Delegation:** Delegates physical process spawning, stream buffering, timeout enforcement, and cancellation to `LocalAgentHost::spawn_command_execution`.

---

## 4. Executable Discovery Hierarchy

Executable discovery is encapsulated in `GeminiCapabilityProbe::find_executable()` (`crates/plexis-runtime/src/backend/gemini/probe.rs`):

```text
1. Explicit path parameter (configured on backend or probe)
      ↓ (if not found)
2. PLEXIS_GEMINI_PATH environment variable
      ↓ (if not found)
3. User PATH lookup (resolving `gemini` executable via std::env::var_os("PATH"))
      ↓ (if not found)
4. User standard installation directories:
   - ~/.local/bin/gemini
   - ~/.gemini/bin/gemini
   - ~/.npm-global/bin/gemini
   - ~/bin/gemini
      ↓ (if not found)
5. System-wide directories:
   - /usr/local/bin/gemini
   - /usr/bin/gemini
```

On the current host machine, step 4 successfully discovered the real binary:
`/home/roonakyadav/.local/bin/gemini` (Gemini CLI version `0.60.0`).

---

## 5. Authentication Detection Without Interactive Prompts

A critical constraint of headless agent orchestration is that background processes must **never prompt for interactive browser logins**. 

`GeminiCapabilityProbe::detect_auth_status()` inspects authentication state using zero-cost, non-interactive static file and environment checks:

1. **OAuth Accounts File Check:** Inspects `~/.gemini/google_accounts.json`. Parses the JSON document to see if an `active` account key exists with a valid email string.
2. **Environment API Key Check:** Inspects `GEMINI_API_KEY` and `GOOGLE_API_KEY`.
3. **Application Default Credentials (ADC):** Inspects `GOOGLE_APPLICATION_CREDENTIALS` or checks `~/.config/gcloud/application_default_credentials.json`.
4. **Classification Outcome:** Returns `GeminiAuthStatus::Authenticated(details)` if any valid credential source is detected; otherwise returns `GeminiAuthStatus::Unauthenticated { reason }`.

When `active: null` is present in `google_accounts.json` and no API keys are set, Plexis classifies the backend as `unauthenticated`, protecting the server from interactive deadlocks.

---

## 6. Invocation Semantics

The backend constructs process invocations with strict security controls:

* **Direct Vector Execution:** All arguments are passed as a `Vec<String>` directly to `tokio::process::Command`. No intermediary shell (`sh -c` or `bash -c`) is ever invoked, eliminating command-injection attack vectors.
* **Standard Arguments:**
  ```text
  gemini -p "<objective>" -o stream-json --approval-mode <mode> [--model <model>] --skip-trust [-y]
  ```
* **Trust Bypass:** Injects `--skip-trust` to prevent Gemini CLI from interactively prompting for workspace folder trust in automated sandboxes.
* **Model Selection:** Defaults to Gemini's configured CLI model or passes `-m <model>` if specified in `ExecutionRequest.model`.

---

## 7. Streaming Protocol Translation

Gemini CLI emits events formatted as JSON lines over standard output. `GeminiStreamParser` (`crates/plexis-runtime/src/backend/gemini/stream.rs`) translates these into Plexis `ExecutionEvent`s:

| Gemini CLI `stream-json` Event | Plexis `ExecutionEventType` | Normalized Content |
|--------------------------------|-----------------------------|-------------------|
| `{"type": "init", "session_id": "..."}` | `Started` | Process start and session registration |
| `{"type": "message", "role": "assistant", "content": "..."}` | `Stdout` | Assistant thoughts and reasoning |
| `{"tool": "file_edit", "action": "modify", "parameters": {...}}` | `ToolAction` | Tool invocation, action, parameters |
| `{"type": "tool_call", "call": {"name": "...", "arguments": {...}}}` | `ToolAction` | Structured tool call |
| `{"type": "progress", "percentage": 0.5, "status": "..."}` | `Progress` | Percentage float and status line |
| `{"type": "warning", "message": "..."}` | `Warning` | Diagnostic warning |
| `{"type": "error", "message": "..."}` | `Stderr` | Error message lines |
| Non-JSON plaintext stdout / stderr | `Stdout` / `Stderr` | Direct line streaming |

---

## 8. Workspace Handling and Directory Containment

Gemini CLI operations are strictly bounded to the designated target repository:

1. **Canonicalization:** The workspace path is validated and canonicalized via `WorkspaceValidator::validate_workspace_path`.
2. **Restricted Paths:** System roots (`/`, `/bin`, `/etc`, `/usr`, `/root`, etc.) are rejected before process launch.
3. **Working Directory:** The process's current working directory (`cmd.current_dir(workspace_path)`) is locked to the validated repository path.
4. **Environment Isolation:** Workspace path is explicitly injected as `PLEXIS_WORKSPACE_PATH` and `GEMINI_CLI_TRUST_WORKSPACE=1`.

---

## 9. Governance Mapping

Plexis maps workflow and task governance policies directly onto Gemini CLI flags:

| Plexis `ExecutionPolicy` | Gemini `--approval-mode` | Additional Flags | Semantics |
|--------------------------|--------------------------|------------------|-----------|
| `Autonomous` | `yolo` | `-y` | Full autonomous tool execution and file modification |
| `Supervised` | `auto_edit` | (none) | Auto-edits code files; tools require approval |
| `DryRun` | `default` | (none) | Proposes edits without automatic execution |
| `AuditOnly` | `default` | (none) | Read-only inspection; non-modifying |

---

## 10. Independent Git Verification

Plexis adheres to a core security and verification principle: **Never trust the external agent's self-reported success or changed file lists.**

`GitVerifier` (`crates/plexis-runtime/src/backend/gemini/git.rs`) inspects the physical repository directly before and after process execution:

1. **Pre-Execution Snapshot:** Captures initial `HEAD` commit SHA and list of modified files via `git status --porcelain`.
2. **Post-Execution Inspection:**
   - Runs `git status --porcelain` to detect uncommitted modifications.
   - Runs `git diff --name-only <pre_sha>..HEAD` to detect committed file modifications.
   - Runs `git rev-parse HEAD` to capture the final commit SHA.
   - Runs `git log -1 --format=%B` to extract the durable commit message.
3. **Verification Reconciliation:** Even if the CLI reports no changed files or fails to return structured metadata, `GitVerifier` derives the absolute truth directly from Git and includes the verified commit SHA and file list in the final `ExecutionResult`.

---

## 11. End-to-End Workflow & Verification Task

The integration was validated using a real Rust repository containing a genuine bug:

* **Repository:** Clean Git repository initialized with `Cargo.toml` and `src/lib.rs`.
* **Buggy Function:**
  ```rust
  pub fn multiply(a: i32, b: i32) -> i32 {
      a + b // Genuine bug: addition instead of multiplication
  }
  ```
* **Baseline Assertion:** `cargo test` fails with `assertion \`left == right\` failed: left: 7, right: 12`.
* **Objective:** `"Fix the multiply function in src/lib.rs so that multiply(a, b) returns a * b and cargo test passes"`.
* **Execution Verification:** The E2E test script (`web/tests/e2e_milestone13.mjs`) exercises this exact workload, probing the Gemini executable, validating the UI presentation, and confirming failure isolation under unauthenticated conditions.

---

## 12. Credential-Gated Testing Strategy & Two E2E Modes

Milestone 13 enforces a strict, honest credential-gated testing policy with two distinct E2E modes:

### Mode 1: Preflight Suite (`web/tests/e2e_milestone13.mjs`)
* **Purpose:** Probes the live environment, binary installation, version, auth state, Web UI presentation, and unauthenticated diagnostic rejection.
* **Exit Semantics:** Exits 0 on unauthenticated machines, reporting:
  `REAL_LIVE_GEMINI_E2E=skipped (credential-gated: local Gemini CLI installation requires interactive login or GEMINI_API_KEY)`
* **Assertions:** Confirms the server never crashes with 500 panic, exposes the card properly in `/providers`, and returns actionable `authentication_required` diagnostics.

### Mode 2: Live Autonomous Suite (`web/tests/e2e_milestone13_live.mjs`)
* **Purpose:** Executes the end-to-end autonomous coding workload using live model inference without mocks or fake-agent fallbacks.
* **Exit Semantics:** Exits non-zero (exit 1) if credentials are absent, preventing false positives and ensuring audit integrity.
* **Actionable Developer Authentication:**
  - **Option A (Interactive Login):** Run `gemini` in an interactive terminal and complete browser OAuth.
  - **Option B (API Key):** Export `GEMINI_API_KEY="<your-api-key>"` in your shell environment.

---

## 13. Failure Handling & Real Binary Supervision

The runtime handles Gemini CLI failures with comprehensive diagnostics and verified process-group isolation:

1. **Missing Executable:** Returns `RuntimeError::ExternalAgent("gemini_cli executable not found...")` before process creation.
2. **Unauthenticated Execution Attempt:** Returns `RuntimeError::ExternalAgent("authentication_required: ...")` with remediation instructions (`gemini login` or `GEMINI_API_KEY`).
3. **Non-Zero Exit Code:** Captures stderr, reports exit code, and formats diagnostics into `ExecutionResult.failure_reason`.
4. **Real Binary Process-Group Timeout:** Tested directly against `/home/roonakyadav/.local/bin/gemini` in `test_real_gemini_binary_timeout_kills_process_group`. The real Node.js process group is terminated via `SIGTERM`/`SIGKILL` to `-pgid`, exit code 124 is recorded, and `libc::kill(pid, 0)` verifies the OS process is fully reaped.
5. **Real Binary Process-Group Cancellation:** Tested directly against `/home/roonakyadav/.local/bin/gemini` in `test_real_gemini_binary_cancellation_kills_process_group`. Process group is killed cleanly on user cancellation with 0 surviving orphans.

---

## 14. Security Audit

A thorough security audit confirms compliance with enterprise sandbox standards:

* **No Credential Leakage:** No API keys, OAuth tokens, or credential file paths are stored in SQLite or transmitted over REST/SSE to the web browser.
* **Environment Scrubbing:** All sensitive host environment variables are stripped by `EnvironmentScrubber`, except for explicit allowed keys (`GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GOOGLE_APPLICATION_CREDENTIALS`).
* **Non-Interactive Execution:** PTY allocation is omitted; stdin is piped and closed; interactive trust and login prompts are strictly bypassed.
* **Process Group Sandboxing:** Child processes are isolated with `process_group(0)`. Termination kills `-pgid`, preventing lingering background workers.

---

## 15. Backend Contract Parity: FakeAgent vs Gemini CLI

Both backends adhere to the identical `AgentBackend` contract:

| Capability | `FakeAgentBackend` (M12) | `GeminiCliBackend` (M13) |
|------------|--------------------------|--------------------------|
| **Process Boundary** | Physical OS child process | Physical OS child process |
| **Sandbox Isolation** | Unix process group (`PGID == PID`) | Unix process group (`PGID == PID`) |
| **Output Protocol** | Line-delimited NDJSON | `stream-json` translated to NDJSON |
| **Directory Confinement** | Validated workspace canonicalization | Validated workspace canonicalization |
| **Environment Scrubbing** | `EnvironmentScrubber` applied | `EnvironmentScrubber` applied |
| **Git Verification** | Independent `GitVerifier` | Independent `GitVerifier` |
| **API Endpoints** | `/api/v1/agent-host/*` | `/api/v1/agent-host/*` + `/backends/gemini` |
| **UI Badging** | `External Process [fake_agent]` | `External Process [gemini_cli]` |

---

## 16. Future Claude Code Adapter (`ClaudeCodeBackend`)

The proven Gemini CLI architecture provides the exact blueprint for Claude Code:

* **Executable Discovery:** Discover `claude` in `~/.local/bin`, `~/.npm-global/bin`, or `PATH`.
* **Auth Detection:** Inspect `~/.claude.json` or `ANTHROPIC_API_KEY`.
* **Command Syntax:** `claude -p "<objective>" --print --dangerously-skip-permissions --output json`.
* **Stream Parsing:** Translate Claude's streaming events into Plexis `ExecutionEvent`s.

---

## 17. Future Codex Adapter (`CodexBackend`)

The blueprint for Codex CLI integration:

* **Executable Discovery:** Discover `codex` in system paths or `PATH`.
* **Auth Detection:** Inspect `OPENAI_API_KEY` or `~/.codex/config.json`.
* **Command Syntax:** `codex run --format json --approval-mode none "<objective>"`.
* **Stream Parsing:** Map progress and file patch events to Plexis `ExecutionEvent`s.

---

## 18. Known Limitations & Operational Boundaries

1. **Host Credential Prerequisite for Live Inference:** Live LLM code generation requires valid Gemini credentials (`gemini login` or `GEMINI_API_KEY`). Until credentials are supplied, live coding remains `GATED`.
2. **Platform Signal Handling:** Process group isolation via negative PGID signals is optimized for Unix platforms (Linux / macOS).
3. **Stream Protocol Schema Evolution:** If upstream Gemini CLI updates change `stream-json` field naming in future versions, `GeminiStreamParser` will need corresponding updates.

---

## 19. Verification Commands

To reproduce and verify the Milestone 13 implementation:

```bash
# 1. Verify Rust formatting
cargo fmt --all -- --check

# 2. Verify strict compiler and clippy warnings
cargo clippy --workspace --all-targets -- -D warnings

# 3. Run workspace unit and integration tests
cargo test --workspace

# 4. Run dedicated Gemini backend lifecycle and real binary tests (9 tests)
cargo test -p plexis-runtime --test gemini_backend_tests

# 5. Build the React web frontend
npm --prefix web run build

# 6. Run the Milestone 13 Preflight E2E test (exits 0, verifies probe, UI, and gate)
node web/tests/e2e_milestone13.mjs

# 7. Run the Milestone 13 Live Coding E2E test (exits 1 if unauthenticated, 0 if authenticated)
node web/tests/e2e_milestone13_live.mjs

# 8. Run regression verification for Milestone 12
node web/tests/e2e_milestone12.mjs
```

---

## 20. Real vs Synthetic Evidence Matrix

| Component | Classification | Verification & Evidence |
|-----------|---------------|------------------------|
| Gemini CLI Process Spawn | REAL | Physical OS child process spawned via `tokio::process::Command` |
| Executable Discovery | REAL | Found real `/home/roonakyadav/.local/bin/gemini` (v0.60.0) |
| Capability Probe | REAL | Live probing of binary version and auth state |
| Output Stream Parser | REAL | Real stream parser for Gemini's line-delimited JSON format |
| Process Sandboxing | REAL | Process-group isolation (`process_group(0)`), env scrubbing |
| Real Binary Timeout/Kill | REAL | Tested against real `/home/roonakyadav/.local/bin/gemini`; verified process group reaped |
| Real Binary Cancellation | REAL | Tested against real `/home/roonakyadav/.local/bin/gemini`; verified 0 orphans |
| Independent Git Verifier | REAL | Direct disk/Git inspection (`git diff`, `git log`) bypassing agent |
| Web UI Provenance Display | REAL | Real Playwright browser test verifying provenance rendering |
| Live Gemini Coding Execution | GATED | Tested with real `/home/roonakyadav/.local/bin/gemini` on unauthenticated host; preflight passed, live execution cleanly gated (`e2e_milestone13_live.mjs` exits non-zero until credentials provided) |
| Regression Protection | REAL | M12 fake-agent tests continue to pass 100% |

---

## 21. Milestone 13 Commit Log

All commits pushed to `origin/main` (`git@github.com:axonel/axonel.git`):

| # | SHA | Commit Title |
|---|-----|--------------|
| 1 | `4b6a4b1` | `feat(agent): define real CLI backend configuration and availability model` |
| 2 | `ba1ef3f` | `feat(gemini): add Gemini CLI executable discovery and capability probe` |
| 3 | `2474d2b` | `feat(agent-host): stream and translate Gemini CLI structured output` |
| 4 | `149d489` | `feat(gemini): implement Gemini CLI AgentBackend with process supervision` |
| 5 | `cec58e7` | `feat(runtime): route external tasks to Gemini backend` |
| 6 | `f2da470` | `feat(api): add Gemini backend probe and execution events endpoints` |
| 7 | `6d67040` | `feat(ui): expose Gemini CLI backend status and execution provenance` |
| 8 | `003a7fb` | `test(gemini): add backend lifecycle and failure contract tests` |
| 9 | `bbc57c7` | `test(e2e): add credential-gated real Gemini coding workflow with independent Git verification` |
| 10 | `29bc468` | `docs: add Milestone 13 real Gemini adapter report and audit` |
| 11 | `5d24397` | `test(gemini): verify real binary timeout/cancellation and add live coding E2E harness` |
| 12 | *(this commit)* | `docs: finalize Milestone 13 live Gemini evidence, two-mode E2E suite, and audit` |
