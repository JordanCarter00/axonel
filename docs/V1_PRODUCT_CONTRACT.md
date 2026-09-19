# Axonel v1 Product Contract: Lifecycle, Release Semantics & Safety Guarantees

**Version:** 1.0.0-rc  
**Status:** Authoritative  
**Domain:** Autonomous Mission Lifecycle, Human Acceptance Gates, Git Integration & Safety Invariants  

---

## 1. Executive Summary & Core Philosophy

Axonel is an autonomous execution substrate for software engineering agents. Unlike raw coding agents that run directly in a developer's working tree, leave uncommitted files, report unverified success, and silently mutate git branches, Axonel enforces a strict, verifiable release contract:

$$\text{Objective} \to \text{Autonomous Execution} \to \text{Independent Verification} \to \mathbf{AwaitingAcceptance} \to \mathbf{Explicit Acceptance} \to \text{Safe Git Integration}$$

### The Cardinal Axiom of Axonel v1
> **Autonomous execution and physical verification NEVER equate to automatic branch integration.**
> Independent verification proves that candidate code compiles, passes tests, and produces valid commits on disk.
> Human acceptance is an independent, inviolable governance gate that must be explicitly granted before any candidate commit is merged into a target repository branch.

---

## 2. The Canonical Mission State Machine

```
   [Created] ──► [Planning] ──► [Running] ──► [Verifying]
                                   │              │
                                   ▼              ▼ (all stopping conditions pass)
                             [NeedsHuman]    [AwaitingAcceptance]
                                   │              │          │
                     (replan) ◄────┘              │          ▼
                        │                         │    [Rejected] (non-destructive audit)
                        ▼                         ▼
                   [Replanning]              [Accepted]
                        ▲                         │
                        │ (continue_mission=true) │ (POST /integrate)
                        └─────────────────────────▼
                                             [Integrated]
```

### State Definitions

| State | Lifecycle Stage | Execution Semantics | Mutability & Transitions |
|---|---|---|---|
| `created` | Pre-execution | Mission declared with objective, budget, and stopping condition. | Transitions to `planning` or `running`. |
| `planning` | Autonomous Plan | Decomposing objective into DAG tasks. | Transitions to `running`. |
| `running` | Autonomous Execution | External agent (e.g. Gemini CLI) executes within isolated worktrees. | Transitions to `verifying`, `replanning`, or `needs_human`. |
| `verifying` | Out-of-Band Audit | Independent compiler and test verifier runs directly on disk (`cargo test = 0`, git commit exists, clean tree). | Transitions to `awaiting_acceptance` on success, or `replanning` on failure. |
| **`awaiting_acceptance`** | **Review Boundary (Halted)** | **Autonomous loop stops cleanly.** Deliverable commit preserved. Agent released. Leases cleared. Mission waits for human review. | Transitions ONLY to `accepted`, `rejected`, `replanning`, or `cancelled`. |
| **`accepted`** | **Operator Approved** | Human developer has reviewed diff and audit evidence and formally accepted the deliverable. Ready for branch integration. | Transitions to `integrated` or `cancelled`. |
| **`integrated`** | **Terminal Delivery** | Candidate commit successfully and cleanly integrated into target repository branch. Target HEAD updated. | **Terminal state.** Idempotent re-calls succeed without duplicate commits. |
| **`rejected`** | **Audited Rejection** | Operator rejected candidate deliverable. Rejection reason recorded durably in event log. Worktrees and candidate commits remain intact for inspection. | **Terminal state** (unless rejected with `continue_mission: true`, which transitions to `replanning`). |
| `needs_human` | Intervention Required | Agent escalated question, ambiguity, or environmental block. | Resumed via human resolution (`resume`, `replan`, `cancel`). |
| `cancelled` | Operator Aborted | Operator explicitly aborted the mission. Running child processes terminated. | Terminal state. |
| `failed` | Failure / Exhaustion | Budget exhausted or unrecoverable error. | Terminal state. |

---

## 3. Strict Transition & API Contract

### Rule 1: Separation of Verification from Acceptance
When physical stopping conditions pass, the mission engine transitions to `MissionState::AwaitingAcceptance`. The background runner breaks execution and removes the mission from active memory. The mission NEVER transitions directly to `Integrated` or `Completed` without explicit operator intent.

### Rule 2: Premature Integration Rejection (HTTP 409 Conflict)
Calling `POST /api/v1/missions/{id}/integrate` while in `awaiting_acceptance` is strictly forbidden:
- **HTTP Status:** `409 Conflict`
- **Error Response:** `"Cannot integrate mission: mission is in state 'awaiting_acceptance'; human acceptance is required before integration. Call POST /api/v1/missions/{id}/accept first."`

### Rule 3: Explicit Human Acceptance (`POST /api/v1/missions/{id}/accept`)
An operator accepts candidate work via the acceptance endpoint:
```json
POST /api/v1/missions/{id}/accept
{
  "feedback": "Approved deliverable for v1 release",
  "integrate": true,
  "target_branch": "main",
  "commit_message": "feat(api): implement concurrent rate gate"
}
```
- **Two-Step Flow:** `integrate: false` transitions to `accepted`. The developer can run additional out-of-band sanity checks and later call `POST /integrate`.
- **Atomic Flow:** `integrate: true` atomically transitions to `accepted` and merges candidate commits into `target_branch`, finishing in `integrated`.

### Rule 4: Rejection Semantics (`POST /api/v1/missions/{id}/reject`)
Rejection is non-destructive and fully auditable:
```json
POST /api/v1/missions/{id}/reject
{
  "reason": "Implementation uses deprecated std::sync::mpsc instead of crossbeam",
  "continue_mission": true
}
```
- **Candidate Commit Preservation:** The candidate commit SHA remains intact in `latest_verified_commit` and on disk in `.git/objects`. No work is deleted.
- **Audit Trail:** An immutable event `mission_rejected` is recorded with the operator's reason.
- **Adaptive Replanning:** If `continue_mission: true`, the mission transitions to `replanning`. The rejection feedback is injected into the next cycle prompt so the agent adapts its solution.

### Rule 5: Idempotency Guarantees
- Replaying `POST /accept` on an already accepted or integrated mission returns `HTTP 200 OK` with the current state.
- Replaying `POST /integrate` on an already integrated mission returns `HTTP 200 OK` with:
  ```json
  {
    "mission_id": "msn_...",
    "integrated": true,
    "already_integrated": true,
    "verified_commit": "abc1234...",
    "integration_summary": "Mission deliverable commit abc1234 was already integrated into main.",
    "timestamp": "..."
  }
  ```
  No duplicate merge commits or redundant git operations are performed.

---

## 4. Git Integration Safety & Clean Rollback

Axonel treats the host repository's branches with extreme caution. Every integration operation enforces four mandatory safety invariants:

### 1. Stale Target Branch Protection (`expected_target_head`)
If concurrent developers pushed commits to the target branch while the agent was working, the integration base is stale.
- The verifier records `verified_target_head` at verification time.
- If target branch `HEAD` moved since verification, `POST /integrate` rejects with `HTTP 409 Conflict`:
  `"Target branch HEAD (<new>) has changed since verification (expected <old>). Please review and re-verify before integrating."`
- Developers may pass `force: true` if fast-forward or 3-way merge is explicitly desired.

### 2. Dirty Target Working Tree Protection
If the developer has uncommitted or dirty files in their active repository tree:
- `POST /integrate` immediately aborts with `HTTP 409 Conflict` before invoking any git merge command:
  `"Target repository working tree is dirty; commit or stash local changes before integration."`
- Local developer modifications are 100% protected from overwrite or clobber.

### 3. Merge Conflict Atomic Rollback (`git merge --abort`)
If git detects a non-fast-forward merge conflict between candidate commits and the target branch:
- Axonel immediately invokes `git merge --abort`.
- The target repository working tree and index are restored to pristine condition.
- No conflict markers (`<<<<<<< HEAD`), partial index entries, or corrupted states are ever left on disk.
- Returns `HTTP 409 Conflict`.

### 4. Verified Commit Immutability
Only commits that have explicitly passed independent, out-of-band verification (`latest_verified_commit`) can be integrated. Unverified or failed missions return `HTTP 409 Conflict`.

---

## 5. Review Package Specification (`GET /api/v1/missions/{id}/review`)

Before making an acceptance decision, operators (and automated tooling) can inspect the comprehensive Review Package:

```json
GET /api/v1/missions/{id}/review
```

### Review Package Schema:
- **`mission_id`**: Canonical UUID.
- **`status`**: Current lifecycle state (`awaiting_acceptance`, `accepted`, `integrated`, `rejected`).
- **`objective`**: Full original developer prompt.
- **`final_commit`**: Verified candidate Git commit SHA.
- **`files_changed`**: Array of files modified between target base and candidate deliverable.
- **`diff_summary`**: `{ files_count: N, insertions: X, deletions: Y }`.
- **`full_diff`**: Complete unified git diff text (`diff --git a/... b/...`).
- **`verification`**: Independent out-of-band test suite status (`tests_passed`, `tree_clean`, `commit_exists`).
- **`duration_secs`**: Total wall-clock execution duration.
- **`total_executions`**: Total agent invocations.
- **`recovery_attempts`**: Self-healing / tool-adaptation count.
- **`warnings`**: Operational warnings (e.g. "Target branch HEAD differs from initial base", dirty files).
- **`audit_timeline`**: Recent chronological audit events with UTC timestamps.
- **`can_accept`**: Boolean flag indicating if operator may call `/accept`.
- **`can_integrate`**: Boolean flag indicating if operator may call `/integrate`.
- **`can_reject`**: Boolean flag indicating if operator may call `/reject`.

---

## 6. Developer Responsibility Accounting

Axonel v1 formally defines and tracks developer effort by separating **Autonomous Execution** from **Release Governance**:

| Operational Phase | Raw Coding Agent (Gemini CLI / Claude / Cursor) | Axonel Autonomous Mission (v1) |
|---|---|---|
| **Objective Dispatch** | 1 action (type prompt) | 1 action (dispatch mission) |
| **Process Monitoring** | Developer must watch terminal or loop | **0 actions** (fully unattended in background) |
| **Error / Crash Recovery** | Developer must diagnose and retry | **0 actions** (autonomous adaptive recovery) |
| **Workspace Pollution** | Direct mutation of primary repo (`Cargo.lock`, caches) | **0 actions** (isolated in disposable worktrees) |
| **Compiler / Test Verification** | Developer must manually invoke `cargo test` | **0 actions** (independent physical verifier) |
| **Candidate Inspection** | Developer must manually `git diff` working tree | 1 action (inspect review package in UI/CLI) |
| **Acceptance & Git Integration** | Developer must manually branch, commit, & merge | 1 action (explicit click or `axonel mission accept --integrate`) |
| **Conflict Recovery** | Developer must resolve aborted merge manually | **0 actions** (clean rollback with `git merge --abort`) |
| **Total Developer Actions During Execution** | **4 manual actions** | **0 manual actions** |
| **Total Release Governance Actions** | **Included in manual triage** | **1 explicit governance action** |

### Summary Guarantee
During autonomous execution, developer intervention is **0**.  
At the release boundary, human governance is **1 explicit action**.  
This guarantees 100% safety without sacrificing autonomous efficiency.
