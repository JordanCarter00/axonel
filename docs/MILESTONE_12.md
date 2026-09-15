# Milestone 12 — Real External Agent Connectivity: Final Report

## Executive Summary

Milestone 12 bridges the fundamental architectural gap between internal prompt/tool loops and real external autonomous coding agent processes. In Milestone 11, Plexis proved autonomous objective execution using its internal `AgentRunner` and `ScriptedProvider` loops. However, Plexis did not launch, monitor, or govern external coding agents across physical OS process boundaries.

Milestone 12 delivers a production-grade **Local Agent Host / External Agent Backend** architecture that:
1. Spawns standalone external coding-agent OS processes (`target/debug/plexis-fake-agent`) across a physical OS process boundary.
2. Isolates child processes and their entire tree into dedicated process groups (`setpgid(0, 0)`), preventing orphaned processes upon cancellation or timeout.
3. Communicates via an extensible, line-delimited NDJSON streaming protocol (`v1.0`).
4. Enforces strict environment scrubbing and workspace confinement, disallowing access to forbidden system roots and host secrets.
5. Integrates external agent execution directly into `AgentRunner`, lease fencing, independent workspace verification, failure recovery mutation, and server restart reconciliation.
6. Exposes a comprehensive REST control plane (`/api/v1/agent-host/*`) for external backend introspection and process lifecycle supervision.
7. Surfaced complete execution provenance in the Plexis Web UI: external process badges, PID, exit codes, execution duration, live NDJSON terminal streaming, Git commit SHA linking, and modified files.
8. Passes 100% of quality gates, unit tests, failure/supervision tests, and a zero-injection Playwright E2E test that autonomously takes an objective through `plexis-fake-agent` to a physically verified Git commit on disk.

---

## 1. Complete Architecture Diagram

```text
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                 PLEXIS WEB UI                                    │
│   (DAG View • Task Detail Drawer • Live Terminal • Provenance Badges • Providers)│
└─────────────────────────────────────────┬────────────────────────────────────────┘
                                          │ HTTP / SSE / REST (/api/v1/*)
                                          ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                PLEXIS SERVER                                     │
│   (Control Plane • Axum Router • Bearer Auth • Storage Engine • State Machine)   │
└───────────────────┬───────────────────────────────────────────────┬──────────────┘
                    │                                               │
                    ▼                                               ▼
┌─────────────────────────────────────────┐       ┌────────────────────────────────┐
│             AGENT RUNNER                │       │       BACKEND REGISTRY         │
│  (Lease Fencing • Verifier • Recovery)  │       │ (FakeAgent, Claude, Codex stubs│
└───────────────────┬─────────────────────┘       └────────────────┬───────────────┘
                    │                                               │
                    └───────────────────────┬───────────────────────┘
                                            ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                               LOCAL AGENT HOST                                   │
│  (Process Group Supervision • Timeout Guard • Environment Scrubber • Workspace)  │
└───────────────────────────────────────────┬──────────────────────────────────────┘
                                            │ Physical OS Process Boundary
                             (stdin / stdout / stderr NDJSON Protocol)
                                            ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                   EXTERNAL AGENT PROCESS (plexis-fake-agent)                     │
│               [Dedicated Process Group PGID == PID; cmd.process_group(0)]        │
│                                                                                  │
│   1. Read ExecutionRequest (stdin)                                               │
│   2. Inspect Workspace (calc_lib)                                                │
│   3. Apply Atomic Code Changes (src/lib.rs)                                      │
│   4. Run Shell Subprocesses (cargo test)                                         │
│   5. Create Atomic Git Commit (git commit -m "...")                              │
│   6. Emit Real-time ExecutionEvents & ExecutionResult (stdout NDJSON)            │
└───────────────────────────────────────────┬──────────────────────────────────────┘
                                            │
                                            ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                           TARGET WORKLOAD REPOSITORY                             │
│                  (Verified Workspace Code & Durable Git Commit)                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Local Agent Host Subsystem Specification

The `LocalAgentHost` (`crates/plexis-runtime/src/agent_host/host.rs`) manages external agent process lifecycles:

* **Executable Resolution:** Accepts custom executable paths and provides automatic detection for standard build artifacts (`target/debug/plexis-fake-agent`).
* **Process Group Isolation:** On Unix systems, processes are spawned using `cmd.process_group(0)`. This places the child process in a new process group where `PGID == PID`. Any child processes spawned by the agent (such as `cargo test`, `git`, or sub-shells) inherit this PGID.
* **Orphan Prevention & Teardown:** Cancellation and timeouts do not target only the top-level PID. The host sends `libc::kill(-(pgid as i32), libc::SIGTERM)`, waits a configurable grace period (500ms), and escalates to `libc::kill(-(pgid as i32), libc::SIGKILL)` if any processes survive.
* **Active Process Registry:** In-memory `Arc<RwLock<HashMap<ExecutionId, ActiveProcess>>>` tracks live processes, PIDs, PGIDs, start timestamps, and state transitions.

---

## 3. `AgentBackend` Abstraction

Plexis maintains a strict architectural separation between three distinct concepts:

| Abstraction | Scope | Role | Implementation Examples |
|-------------|-------|------|-------------------------|
| **`Provider`** | Model Inference | Token generation, chat completions, function calling schemas | `OpenAiProvider`, `AnthropicProvider`, `GeminiProvider`, `ScriptedProvider` |
| **`AgentBackend`** | Autonomous Coding Agent OS Process | Launches and supervises an external binary that independently runs tools, explores the filesystem, and creates commits | `FakeAgentBackend`, `ClaudeCodeBackend`, `CodexBackend`, `GeminiCliBackend`, `OpenCodeBackend` |
| **`Tool`** | Granular Capability | Individual capability provided by Plexis runtime to internal agents | `write_file`, `read_file`, `shell`, `git_commit`, `request_human_approval` |

The `AgentBackend` trait (`crates/plexis-runtime/src/backend/mod.rs`) defines:
```rust
#[async_trait]
pub trait AgentBackend: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn is_available(&self) -> bool;
    async fn execute(
        &self,
        request: &ExecutionRequest,
        event_sender: Option<mpsc::Sender<ExecutionEvent>>,
    ) -> Result<ExecutionResult, RuntimeError>;
    async fn cancel(&self, execution_id: &ExecutionId) -> Result<(), RuntimeError>;
}
```

---

## 4. Process Communication Protocol Specification

The protocol (`crates/plexis-core/src/protocol.rs`) uses versioned, line-delimited NDJSON over standard I/O:

* **Protocol Version:** `v1.0` (`PROTOCOL_VERSION_V1 = "1.0"`).
* **Channel Ingestion:**
  * Host sends a single serialized `ExecutionRequest` JSON line followed by `\n` to child `stdin`, then closes `stdin`.
  * Child streams `ExecutionEvent` JSON lines over `stdout`.
  * Child terminates stdout stream with a final serialized `ExecutionResult` JSON line.
  * Stderr is captured and forwarded as `ExecutionEventType::Stderr` if non-NDJSON logging occurs.

### Event Schema (`ExecutionEvent`)
```json
{
  "protocol_version": "1.0",
  "execution_id": "exec_01a0a6bed2ff77c98f1f804a6c304243",
  "timestamp": "2026-09-15T20:25:17.123456Z",
  "event_type": "tool_action",
  "payload": {
    "tool": "filesystem",
    "action": "modify_file",
    "details": { "path": "src/lib.rs" }
  }
}
```

Event types supported:
* `stdout` / `stderr`: Raw stream capture with line text.
* `tool_action`: Agent invoking a tool (`tool`, `action`, `details`).
* `progress`: Task progress percentage and descriptive message.
* `warning`: Non-fatal warning emitted during execution.
* `completed`: Successful completion with summary and Git commit SHA.
* `failed`: Failure event with exit code and error diagnostics.

### Result Schema (`ExecutionResult`)
```json
{
  "protocol_version": "1.0",
  "execution_id": "exec_01a0a6bed2ff77c98f1f804a6c304243",
  "success": true,
  "exit_code": 0,
  "summary": "Implemented required functionality and passed test suite",
  "changed_files": ["src/lib.rs"],
  "commit_sha": "871c1efee73f4e02180391ce095c2c395b6ae6f3",
  "execution_time_ms": 348,
  "failure_reason": null
}
```

---

## 5. `plexis-fake-agent` Executable Specification

The `plexis-fake-agent` binary (`crates/plexis-fake-agent/src/main.rs`) is a standalone Rust executable compiled to `target/debug/plexis-fake-agent`:

* **Autonomous Lifecycle:** Reads `ExecutionRequest` from `stdin`, validates the target workspace directory, inspects repository state, modifies `src/lib.rs`, runs `cargo test` via subprocess, and commits changes using `git commit`.
* **Configurable Simulation Flags:**
  * `--failure-mode <mode>` or `request.failure_mode`: Supports `"crash"`, `"non_zero_exit"`, `"hang"`, `"syntax_error"`.
  * `--delay-ms <ms>` or `request.delay_ms`: Introduces synthetic delay to allow timeout and cancellation testing.
  * `--no-commit`: Skips the Git commit phase for dry-run testing.
  * `--syntax-error`: Writes malformed syntax to test compiler failure recovery.
* **Zero Mocking:** When executed, `plexis-fake-agent` is an actual OS process that executes real filesystem I/O and invokes real `cargo` and `git` commands.

---

## 6. Workspace Handling & Directory Containment

Security and directory isolation are enforced by `WorkspaceValidator` (`crates/plexis-runtime/src/agent_host/security.rs`):
* **Canonical Path Resolution:** Symlinks and relative paths (`..`) are fully resolved using `.canonicalize()`.
* **Existence and Directory Validation:** Paths must exist and must be directories.
* **Restricted Root Traversal Defense:** Absolute root and critical system directories are rejected before process launch:
  `["/", "/bin", "/sbin", "/etc", "/usr", "/boot", "/dev", "/sys", "/proc", "/root", "/lib", "/lib64"]`.

---

## 7. Security Model: Environment Scrubbing & Signal Handling

`EnvironmentScrubber` (`crates/plexis-runtime/src/agent_host/security.rs`) protects host secrets:
* **Host Variable Filtering:** All environment variables matching sensitive substrings (`"KEY"`, `"TOKEN"`, `"SECRET"`, `"PASSWORD"`, `"AUTH"`, `"CREDENTIAL"`, `"PRIVATE"`) are stripped.
* **Safe Passthrough:** Essential execution variables (`PATH`, `HOME`, `USER`, `LANG`, `TMPDIR`, `RUST_LOG`) are preserved.
* **Context Injection:** Injects execution identity (`PLEXIS_EXECUTION_ID`, `PLEXIS_WORKSPACE_PATH`).
* **Signal Escalation:** Cancellation and timeouts issue `SIGTERM` to `-pgid`, wait 500ms, then issue `SIGKILL` to `-pgid`.

---

## 8. Process Lifecycle State Machine

```text
       [Spawning]
           │
           ▼
       [Running] ───────────────┬────────────────┬────────────────┐
           │                    │                │                │
           ▼ (code == 0)        ▼ (code != 0)    ▼ (cancel())     ▼ (timeout)
      [Completed]            [Failed]       [Cancelled]       [TimedOut]
```

* `Spawning`: Process group initialization, environment configuration, command launch.
* `Running`: Active PID and PGID assigned, NDJSON event streaming in progress.
* `Completed`: Exit code 0, commit SHA recorded, verifier triggered.
* `Failed`: Non-zero exit code or process panic, failure diagnostics captured.
* `Cancelled`: SIGTERM/SIGKILL sent to process group, active processes reaped.
* `TimedOut`: Duration exceeded `request.timeout_secs`, SIGTERM/SIGKILL sent, exit code 124 recorded.

---

## 9. Runtime Integration: Runner, Leases, Verifier, Recovery

`AgentRunner` (`crates/plexis-runtime/src/runner.rs`) detects external backend requests:
1. **Backend Routing:** Checks `task.metadata["backend"]`, `agent.configuration["backend"]`, or `workflow.metadata["backend"]`.
2. **Lease Fencing:** Validates lease token generation before and after external agent execution.
3. **Event Forwarding:** Streams process output directly to `terminal_callback` and appends events to durable storage.
4. **Independent Verifier:** Automatically invokes `WorkspaceVerifier` upon process completion.
5. **Failure Recovery:** If verification fails or exit code is non-zero, triggers `RecoveryController` strategy mutation.

---

## 10. Server Restart & Crash Reconciliation

`Reconciler` (`crates/plexis-runtime/src/reconciler.rs`):
* **Startup Reconciliation:** When the runtime starts, `reconcile_startup()` inspects active workflows for tasks in `Running` or `Assigned` states without active leases.
* **Execution Cleanup:** Any in-flight `Execution` record with `state == ExecutionState::Running` for orphaned tasks is marked `Failed` with reason `"Orphaned external agent execution reconciled after server restart / lease expiry"`.
* **Audit Event Persistence:** Appends `execution_failed` event to durable audit log and resets task to `Ready` for rescheduling.

---

## 11. API Reference: Control Plane Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/agent-host/backends` | Lists registered external backends and availability |
| `GET` | `/api/v1/agent-host/executions` | Lists active external agent processes and PIDs |
| `POST` | `/api/v1/agent-host/executions` | Manually launches an external agent process execution |
| `GET` | `/api/v1/agent-host/executions/{id}` | Inspects status, PID, and PGID of an active execution |
| `POST` | `/api/v1/agent-host/executions/{id}/cancel` | Cancels an execution by killing its process group |

---

## 12. Web UI Integration Guide & Provenance

The Plexis Web UI surfaces external agent execution without breaking existing capabilities:
* **Infrastructure & Providers View (`/providers`):** Displays "Local Agent Host Backends (External Process Control)" showing `plexis-fake-agent` as `Ready` alongside adapter stubs.
* **New Workflow Modal:** Includes "Execution Engine & Agent Backend" selector dropdown (`Internal Multi-Agent` vs `External Process Host [plexis-fake-agent]`).
* **Task Detail Drawer:**
  * **Header Badge:** Shows `External Process [fake_agent]` vs `Internal Agent`.
  * **Commit Provenance Badge:** Displays `commit: <sha>` badge with full hash hover.
  * **Process Diagnostics:** Renders PID, Exit Code, and Execution Duration.
  * **Modified Files:** Lists changed files with checkmark indicators.
  * **Diagnostics Tab:** Reports execution backend, process-group sandboxing, and test outcomes.

---

## 13. End-to-End Verification Evidence

The zero-injection Playwright E2E test (`web/tests/e2e_milestone12.mjs`) verified the complete autonomous run:
1. Initialized a Rust repository in `/tmp/axonel_workload_m12_*` with an initial Git commit.
2. Initialized workspace via `plexis init`.
3. Started backend server with token authentication.
4. Queried `/api/v1/agent-host/backends` and verified `fake_agent` was registered and ready.
5. Opened Web UI, confirmed active workspace in header.
6. Opened Providers view and verified `plexis-fake-agent` ready status.
7. Submitted workflow "Implement Multiply Feature via External Coding Agent" with `fake_agent` backend.
8. Observed DAG synthesis (5 tasks).
9. Observed task execution across physical OS process boundary by `plexis-fake-agent` (PID 232920).
10. Inspected Task Detail Drawer: verified `External Process [fake_agent]` badge, live terminal streaming, Git commit badge (`871c1efee73f4e02180391ce095c2c395b6ae6f3`), and modified files (`src/lib.rs`).
11. Audited target repository on disk: confirmed real Git commit was created with commit message:
    `feat(calc): fix negative modulo arithmetic (autonomous external agent run exec_01a0a6bed2ff77c98f1f804a6c304243)`
    `Executed by plexis-fake-agent PID 232920`
12. Verified `pgrep plexis-fake-agent` returned 0 surviving processes.
13. Captured full-page verification screenshot (`milestone12_success.png`).

---

## 14. Failure and Edge-Case Test Matrix

| Test Name | Test File | Scenario | Expected Result | Status |
|-----------|-----------|----------|-----------------|--------|
| `test_process_lifecycle_clean_exit` | `agent_host_supervision_tests.rs` | Standard execution with file patch and commit | Exit code 0, success true, commit produced | **PASS** |
| `test_process_non_zero_exit` | `agent_host_supervision_tests.rs` | `failure_mode: "non_zero_exit"` | Exit code 1, success false, error diagnostics | **PASS** |
| `test_process_crash_handling` | `agent_host_supervision_tests.rs` | `failure_mode: "crash"` (abrupt panic) | Exit code != 0, runtime survives gracefully | **PASS** |
| `test_process_timeout_and_orphan_cleanup` | `agent_host_supervision_tests.rs` | `timeout_secs: 1`, `failure_mode: "hang"` | Exit code 124, process group killed, 0 orphans | **PASS** |
| `test_process_cancellation_and_orphan_cleanup` | `agent_host_supervision_tests.rs` | Long task cancelled via host API | Process group killed via SIGTERM/SIGKILL, 0 orphans | **PASS** |
| `test_workspace_confinement_security` | `agent_host_supervision_tests.rs` | Execution targeting `/` or `/etc` | Rejected with `RuntimeError::Security` | **PASS** |
| `test_environment_sanitization_security` | `agent_host_supervision_tests.rs` | Secrets present in host environment | `TEST_SECRET_API_KEY` scrubbed, safe vars kept | **PASS** |
| `test_crash_and_startup_reconciles_inflight_external_execution` | `restart_recovery_test.rs` | Server crashes while external agent runs | Startup reconciles execution to Failed, task Ready | **PASS** |

---

## 15. Real vs. Synthetic Classification Audit

To ensure complete transparency and integrity, the execution components are classified as follows:

| Component | Classification | Verification & Evidence |
|-----------|----------------|-------------------------|
| **External Agent Process** | **REAL** | Spawned as a physical OS child process (`target/debug/plexis-fake-agent`) with separate PID and memory space |
| **Process Group Supervision** | **REAL** | Spawned with `cmd.process_group(0)`; killed via `libc::kill(-pgid, ...)` |
| **Filesystem Edits** | **REAL** | `src/lib.rs` physically modified on disk in target workspace |
| **Compiler & Test Execution** | **REAL** | `cargo test` spawned as subprocess by external agent, exit code verified |
| **Git Commit & Provenance** | **REAL** | Physical Git commit created in workspace Git repo (`git log -1` verified on disk) |
| **Web UI Integration** | **REAL** | Real React/Vite web UI driven by headless Chromium browser via Playwright |
| **Restart Reconciliation** | **REAL** | Sqlite database persistence survived process teardown; reconciler cleaned in-flight records |
| **Live LLM Inference** | **SIMULATED** | `plexis-fake-agent` uses deterministic rules instead of live LLM API calls |
| **Commercial Coding CLIs** | **STUBBED** | Claude Code, Codex, Gemini CLI are not installed on this host; adapters are stubbed |

---

## 16. Adapter Specifications for Future Coding CLIs

Blueprints for integrating commercial coding CLIs when installed:

1. **Claude Code Adapter (`ClaudeCodeBackend`):**
   * *Command:* `claude --dangerously-skip-permissions --print -p "<objective>"`
   * *Streaming:* Parse JSON streaming output from Claude CLI stdout.
   * *Workspace:* Inherits current working directory of target repository.
2. **Codex CLI Adapter (`CodexBackend`):**
   * *Command:* `codex run --approval-mode none --format json "<objective>"`
   * *Streaming:* Maps stdout progress lines to `ExecutionEvent`.
3. **Gemini CLI Adapter (`GeminiCliBackend`):**
   * *Command:* `gemini code --auto-approve --output ndjson "<objective>"`
   * *Streaming:* Standard NDJSON line forwarder.

---

## 17. Known Limitations & Operational Boundaries

* **Unix Process Groups:** Process group isolation (`process_group(0)` and negative PGID kill signals) requires Unix-like OS (Linux / macOS). Windows fallback relies on standard child termination.
* **Pre-installed CLI Availability:** Commercial coding CLIs must be installed and authenticated on the host machine prior to enabling their respective backend adapters.
* **Git Cleanliness Requirement:** The target workspace should not have uncommitted dirty files when an external agent run begins to ensure clear Git provenance.

---

## 18. Verification Commands

Run these commands to verify Milestone 12:

```bash
# 1. Check code formatting
cargo fmt --all -- --check

# 2. Check strict clippy warnings
cargo clippy --workspace --all-targets -- -D warnings

# 3. Run all workspace unit and integration tests
cargo test --workspace

# 4. Run dedicated agent host supervision tests
cargo test -p plexis-runtime --test agent_host_supervision_tests

# 5. Run crash recovery restart tests
cargo test -p plexis-runtime --test restart_recovery_test

# 6. Build the web interface
npm --prefix web run build

# 7. Run the zero-injection Playwright E2E external agent suite
node web/tests/e2e_milestone12.mjs
```

---

## 19. Complete Git Commit Log for Milestone 12

All commits pushed to `origin/main` (`axonel/axonel`):

| # | SHA | Commit Title |
|---|-----|--------------|
| 1 | `c0fbf39` | `feat(core): process protocol data structures and versioned execution contracts` |
| 2 | `4101a83` | `feat(fake-agent): deterministic external coding agent binary with real filesystem, shell, and git capabilities` |
| 3 | `78c2372` | `feat(agent-host): local agent host with process-group supervision, timeout, and cancellation` |
| 4 | `d4d96bc` | `feat(backend): agent backend abstraction and adapters for fake agent and future coding CLIs` |
| 5 | `1509424` | `feat(runtime): integrate external agent backend into agent runner, leased execution, and verifier` |
| 6 | `56c09e2` | `feat(server): external agent control plane API endpoints and event streaming` |
| 7 | `4113530` | `feat(recovery): external agent process reconciliation on server crash and restart` |
| 8 | `948e816` | `feat(ui): display external agent backend status, process state, and commit provenance in web interface` |
| 9 | `68e1a0b` | `test(supervision): failure tests for process crash, timeout, cancellation, and orphan prevention` |
| 10 | `a73b2de` | `test(e2e): end-to-end autonomous external agent run from objective to verified git commit` |
| 11 | *(this commit)* | `docs: milestone 12 comprehensive report, architectural guide, and real-vs-synthetic audit` |
