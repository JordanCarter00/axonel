# Milestone 19: Release Semantics — Human Acceptance, Safe Integration & Rollback

**Milestone Status:** Completed & Verified  
**Date:** September 19, 2026  
**Artifacts Generated & Updated:**
- `docs/V1_PRODUCT_CONTRACT.md`: Authoritative specification of the Axonel v1 product contract, lifecycle transitions, git integration invariants, and rollback semantics.
- `web/tests/m19_acceptance_tests.mjs`: Comprehensive automated test suite verifying all 15 policy guarantees (Scenarios A through O) with 100% pass rate.
- `crates/plexis-core/src/state.rs`: Extended `MissionState` domain model with explicit acceptance states (`AwaitingAcceptance`, `Accepted`, `Integrated`, `Rejected`).
- `crates/plexis-runtime/src/mission/engine.rs`: Enforced physical stopping condition transition to `AwaitingAcceptance`, halted runner cycles, and isolated candidate worktrees.
- `crates/plexis-server/src/routes.rs`: Implemented Review Package endpoint (`GET /review`), Human Acceptance endpoint (`POST /accept`), Non-destructive Rejection (`POST /reject`), and 409 Conflict rejection for unaccepted integration attempts.
- `crates/plexis-server/src/git.rs`: Hardened git integration with `expected_target_head` freshness verification, dirty working tree pre-check, and atomic rollback via `git merge --abort`.
- `crates/plexis-server/src/cli.rs`: Added CLI commands (`plexis mission review`, `plexis mission accept`, `plexis mission reject`, `plexis mission integrate`).
- `web/src/components/MissionsView.tsx`: Implemented interactive Web UI Review Package modal, unified diff viewer, verification badge cards, and rejection dialog.
- `web/tests/e2e_honest_validation.mjs`: Aligned benchmark telemetry to cleanly report 0 developer actions during autonomous execution and 1 explicit action at the release acceptance gate.

---

## 1. Context & The Milestone 18 Contradiction

Milestone 18 established validation integrity and hardened the Axonel substrate against raw Gemini CLI baselines. However, it exposed a critical product ambiguity:

Our conceptual product flow was defined as:
$$\text{Objective} \to \text{Autonomous Execution} \to \text{Independent Verification} \to \text{Developer Reviews Result} \to \text{Developer Explicitly Accepts} \to \text{Integration}$$

Yet the Milestone 18 benchmark reported **0 developer actions** while missions were already integrated into the target repository branch. This created an ambiguous and dangerous mental model:
- Did Axonel silently merge code into `main` without human permission?
- Or did the benchmark conflate unattended agent execution with human release governance?

Leaving these two models ambiguous before public v1 release would undermine user trust. A developer must never wonder whether an autonomous agent is silently committing code to their production branch behind their back.

---

## 2. The Canonical v1 Release Policy

Milestone 19 resolves this ambiguity by defining and implementing **ONE canonical v1 behavior**:

$$\text{Created} \to \text{Planning} \to \text{Running} \to \text{Verifying} \to \mathbf{AwaitingAcceptance} \to \mathbf{Accepted} \to \mathbf{Integrated}$$

### Core Architectural Invariants:

1. **Strict Verification vs Acceptance Separation:**
   - Physical verification (`cargo test = 0`, valid git commit on disk, clean worktree) proves that candidate code is correct on disk.
   - Upon verification passing, execution halts at `MissionState::AwaitingAcceptance`.
   - Autonomous background cycles stop immediately. Runtime leases are cleared. Candidate commits remain isolated.
   - **Autonomous missions NEVER silently integrate into the target branch.**

2. **Premature Integration Rejection (HTTP 409 Conflict):**
   - Calling `POST /api/v1/missions/{id}/integrate` while in `awaiting_acceptance` returns `HTTP 409 Conflict` with clear diagnostic guidance:  
     `"Cannot integrate mission: mission is in state 'awaiting_acceptance'; human acceptance is required before integration. Call POST /api/v1/missions/{id}/accept first."`

3. **Explicit Human Acceptance (`POST /api/v1/missions/{id}/accept`):**
   - The operator inspects the Review Package and issues an explicit acceptance decision.
   - Calling `/accept` with `integrate: false` moves the mission to `accepted`.
   - Calling `/accept` with `integrate: true` atomically accepts and integrates into the target branch.

4. **Non-Destructive Rejection (`POST /api/v1/missions/{id}/reject`):**
   - Rejecting a deliverable requires a mandatory explanation recorded in the immutable audit log.
   - Candidate commits and worktrees remain completely intact on disk.
   - If `continue_mission: true` is requested, the mission transitions to `replanning` with human feedback injected into the next cycle.

5. **Idempotency Guarantees:**
   - Repeated calls to `/accept` return `HTTP 200 OK` with the current state.
   - Repeated calls to `/integrate` return `HTTP 200 OK` with `already_integrated: true`, creating no duplicate commits.

6. **Target Branch Freshness (`expected_target_head`):**
   - If concurrent commits moved the target branch HEAD since verification, integration is rejected with `HTTP 409 Conflict` to prevent silent clobbering or stale rebases. `force: true` is available if fast-forward is explicitly desired.

7. **Clean Rollback on Merge Conflict:**
   - If a merge conflict arises during integration, Axonel immediately invokes `git merge --abort`, restoring the working tree and index to pristine condition with zero conflict markers or untracked debris.

---

## 3. Automated Verification: The 15 Release Semantics Scenarios

We authored and verified an automated test suite (`web/tests/m19_acceptance_tests.mjs`) covering every policy guarantee:

| Scenario Code | Guarantee Tested | Result | Verification Evidence |
|---|---|---|---|
| **Scenario A** | Verified mission stops at review boundary (`awaiting_acceptance`) without integrating | **PASS** | Target branch HEAD untouched; candidate commit isolated on candidate branch. |
| **Scenario B** | Direct integration without acceptance rejected | **PASS** | `POST /integrate` returns HTTP 409 Conflict with actionable message. |
| **Scenario C1** | Two-step acceptance transitions to `accepted` | **PASS** | `POST /accept` with `integrate: false` transitions state to `accepted`. |
| **Scenario C2** | Subsequent integration succeeds | **PASS** | `POST /integrate` transitions to `integrated` and fast-forwards target HEAD. |
| **Scenario D** | Atomic acceptance and integration | **PASS** | `POST /accept` with `integrate: true` updates state to `integrated` atomically. |
| **Scenario E** | Non-destructive rejection preserves commits & audit | **PASS** | Transitions to `rejected`, candidate commit exists on disk, reason recorded. |
| **Scenario F** | Rejection with `continue_mission: true` replans | **PASS** | Transitions to `replanning`, feedback captured in cycle metadata. |
| **Scenario G** | Merge conflict atomic rollback | **PASS** | Returns HTTP 409 Conflict, executes `git merge --abort`, working tree pristine. |
| **Scenario H** | Dirty target branch rejected | **PASS** | Returns HTTP 409 Conflict, local uncommitted modifications untouched. |
| **Scenario I** | Stale target branch (`expected_target_head` mismatch) rejected | **PASS** | Target moved ahead returns HTTP 409 Conflict without modifying target. |
| **Scenario J** | Idempotency of `/accept` | **PASS** | Second `/accept` call returns HTTP 200 OK with current state. |
| **Scenario K** | Idempotency of `/integrate` | **PASS** | Second `/integrate` call returns HTTP 200 OK with `already_integrated: true`. |
| **Scenario L** | Server crash/restart in `awaiting_acceptance` | **PASS** | Restores state cleanly as `awaiting_acceptance` without resuming runner cycles. |
| **Scenario M** | Server crash/restart in `accepted` | **PASS** | Restores state cleanly as `accepted`, ready for integration. |
| **Scenario N** | Cancel while awaiting acceptance | **PASS** | `POST /cancel` transitions cleanly to `cancelled`. |
| **Scenario O** | Comprehensive Review Package delivery | **PASS** | `GET /review` returns unified diff, changed files, metrics, and action flags. |

**Test Result:** **15 / 15 PASSED (100%)**

---

## 4. Honest Developer Effort Accounting

With Milestone 19, the benchmark metrics are mathematically and operationally unambiguous:

| Operational Metric | Baseline A (Raw Agent) | Axonel v1 (Autonomous Mission) |
|---|---|---|
| **Autonomous Execution Actions** | 4 manual actions (watch terminal, diagnose dirty tree, test, commit) | **0 manual actions** (fully unattended in isolated worktree) |
| **Release Acceptance Actions** | Included in manual triage | **1 explicit action** (review deliverable package & click accept) |
| **Silent Target Branch Mutation** | Yes (runs in active working tree) | **NEVER** (hard gate at `awaiting_acceptance`) |
| **Independent Physical Verification** | None (agent self-reports) | **Enforced** (`cargo test = 0`, commit exists on disk) |
| **Merge Conflict Hygiene** | Manual conflict markers on disk | **Atomic rollback** (`git merge --abort`, clean tree) |

---

## 5. Conclusion & Release Readiness

Milestone 19 solidifies the human-in-the-loop contract for Axonel. Autonomous agents provide massive speed and scale by eliminating 100% of execution friction (0 developer actions during execution), while engineering teams retain absolute authority over repository releases (1 explicit acceptance gate).

Axonel v1 release semantics are verified, safe, and ready for general distribution.
