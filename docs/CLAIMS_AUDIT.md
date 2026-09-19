# Axonel Claims Audit & Truthfulness Ledger

**Version:** 1.0  
**Milestone:** 21 (Public Release Hardening)  
**Status:** Canonical Audit  

This ledger classifies all claims made in Axonel documentation, CLI descriptions, and marketing materials into four standardized truthfulness categories:
- **SUPPORTED:** Empirically proven by automated regression tests or end-to-end suites.
- **PARTIALLY SUPPORTED:** Functional under documented prerequisites or local configurations.
- **NOT YET VALIDATED:** Scaffolded in codebase or planned for future integration.
- **UNSUPPORTED / REMOVED:** Misleading claims that have been explicitly retracted and purged from documentation.

---

## 1. Summary Classification Matrix

| Dimension | Claim Statement | Status | Evidence / Test Reference |
| :--- | :--- | :--- | :--- |
| **Durability** | State survives process crash / restart without losing unmerged work | **SUPPORTED** | Tested in `m19_acceptance_tests.mjs` & `m20_integration_reliability_tests.mjs` |
| **Git Safety** | Candidate commits are confined to isolated worktrees; primary tree is never edited directly | **SUPPORTED** | `plexis-runtime/src/worktree.rs`, verified in M19 & M20 suites |
| **Reconciliation** | Daemon reconciles intermediate `Integrating` states on restart using Git as truth | **SUPPORTED** | `reconcile_startup()`, verified in M20 Scenarios C & D |
| **Human Boundary** | Integration is strictly blocked until explicit human review & acceptance | **SUPPORTED** | Verified in M19 Scenario A & M20 Scenarios A, B, N |
| **External Agent** | Google Gemini CLI runs as external OS process and completes end-to-end fixes | **SUPPORTED** | `GeminiCliBackend`, proven in M20 Scenario O & M18 benchmarks |
| **Security Defaults** | Default bind is loopback (`127.0.0.1`); non-loopback bind without auth fails startup | **SUPPORTED** | `crates/plexis-server/tests/security_tests.rs` (Tests A through F) |
| **Workspace Locking** | Concurrent integration requests on the same workspace are intra-process serialized | **SUPPORTED** | `WorkspaceLockManager`, verified in M20 Scenarios E & F |
| **Conflict Rollback** | Merge conflicts cleanly abort integration and roll back state to `Accepted` | **SUPPORTED** | Verified in M20 Scenario H |
| **Stale Target Guard** | Target branch drift triggers re-verification warning and prevents silent overwrites | **SUPPORTED** | Verified in M20 Scenario J |
| **Multi-Provider Hub** | Supports Claude Code, Codex, and OpenCode out of the box | **NOT YET VALIDATED** | Only scaffold adapter stubs exist (`adapters.rs`); labeled as stubs |
| **Privacy & Egress** | "100% Local (Zero Code Egress)" | **UNSUPPORTED / REMOVED** | **Retracted.** External LLMs receive prompt and code context. Control plane is local. |
| **Developer Effort** | "Zero Babysitting (Walk Away)" | **UNSUPPORTED / REMOVED** | **Retracted.** Replaced with "Supervised autonomous background execution with human sign-off." |
| **Verification Authority** | "Guaranteed correct / Formal proof of correctness" | **UNSUPPORTED / REMOVED** | **Retracted.** "Verified" strictly denotes passing configured tests and stopping conditions. |
| **Productivity Metrics** | "Saves 90% of developer time" | **UNSUPPORTED / REMOVED** | **Retracted.** M18 metrics represent synthetic benchmark iteration accounting, not human studies. |

---

## 2. Detailed Analysis of Retracted Claims

### Claim 1: "100% Local (Zero Code Egress)"
- **Previous Statement:** Marketing table claimed Axonel provided "100% Local (Zero Code Egress)" compared to cloud agents.
- **Why Unsupported:** When developers select Google Gemini CLI (or any external LLM provider), repository code snippets, compiler outputs, and objective prompts are transmitted over HTTPS to the provider's API.
- **Corrected Position:** The Axonel supervisor daemon, SQLite state store, Git worktrees, and verifier run 100% locally on the developer's machine. Data egress to LLM providers is governed by the specific provider backend chosen by the user.

### Claim 2: "Zero Babysitting (Walk Away)"
- **Previous Statement:** Positioned Axonel as requiring "Zero (Walk Away)" human involvement.
- **Why Unsupported:** Autonomous agents frequently encounter ambiguity, test failures, or design trade-offs. Milestone 19 introduced mandatory human acceptance boundaries (`AwaitingAcceptance`) precisely because human review is essential before merging code to production branches.
- **Corrected Position:** Axonel enables "supervised autonomous background execution": the agent executes and verifies in the background, but human engineers retain explicit governance over deliverable review and integration.

### Claim 3: "Proof of Correctness"
- **Previous Statement:** Referring to verified deliverables as "proven correct".
- **Why Unsupported:** Passing a compiler and unit test suite does not constitute mathematical proof of correctness. Tests can be incomplete, flaky, or specify incorrect assertions.
- **Corrected Position:** In Axonel, "Verified" has a precise, falsifiable definition: **all configured stopping conditions (compiler exit code == 0, required test suite pass count == 100%, clean working tree) passed against independently observed disk state**.

---

## 3. Provider & Agent Classification

The `/api/v1/agent-host/backends` API and UI truthfully report the status of all registered agent backends:

1. **`gemini_cli`**: `support_tier: "implemented"` (requires Gemini CLI installation and credentials). Dynamic capability probe verifies binary presence and authentication status.
2. **`fake_agent`**: `support_tier: "test_only"`. Deterministic test double used for integration testing and pipeline validation.
3. **`claude_code`**: `support_tier: "stub"`, `probe_status: "unavailable"`. Future integration scaffold.
4. **`codex`**: `support_tier: "stub"`, `probe_status: "unavailable"`. Future integration scaffold.
5. **`opencode`**: `support_tier: "stub"`, `probe_status: "unavailable"`. Future integration scaffold.
