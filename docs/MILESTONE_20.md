# Milestone 20: Product Reliability — True Transactional Integration, Recovery Guarantees & Real-World Repository Safety

**Milestone Status:** Completed & Verified  
**Date:** September 19, 2026  
**Artifacts Generated & Updated:**
- `docs/MILESTONE_20.md`: Comprehensive milestone report on transactional integration, crash recovery, and repository safety.
- `docs/INTEGRATION_STATE_MACHINE.md`: Formal specification of the integration state machine, crash recovery truth table, and concurrency model.
- `docs/V1_PRODUCT_CONTRACT.md`: Authoritative update documenting the `Integrating` intermediate state, Git-authoritative reconciliation, and truthfulness guarantees.
- `docs/RELEASE_CANDIDATE.md`: Updated release candidate certification with M20 reliability results.
- `crates/plexis-core/src/state.rs`: Added `MissionState::Integrating` variant, transition rules, and query semantics.
- `crates/plexis-server/src/workspace_lock.rs`: Implemented `WorkspaceLockManager` providing per-workspace intra-process mutex serialization.
- `crates/plexis-server/src/git.rs`: Hardened git operations with `is_commit_ancestor` (`git merge-base --is-ancestor`) and `abort_merge_if_in_progress`.
- `crates/plexis-server/src/routes.rs`: Integrated `Integrating` lifecycle, durable intent logging, workspace mutex acquisition, rollback to `Accepted`, and review package target head freshness.
- `crates/plexis-runtime/src/reconciler.rs`: Extended startup and recovery reconciler to inspect physical Git disk state for `Integrating` missions and resolve truth.
- `crates/plexis-server/src/cli.rs`: Added `plexis reconcile` CLI command and automatic startup reconciliation.
- `web/src/types/index.ts` & `web/src/components/MissionsView.tsx`: Added `integrating` state badge and target branch freshness alert banner in Review modal.
- `web/tests/m20_integration_reliability_tests.mjs`: Authored dedicated 15-scenario reliability test suite (14 Domain/Policy + 1 Real Gemini E2E) with 100% pass rate.

---

## 1. Audit of the "Atomic Accept + Integrate" Claim

In Milestone 19, the documentation and API contracts used the phrase "atomically accepts and integrates".

Milestone 20 began with a rigorous audit of this claim:
- **SQLite and Git are separate physical persistence engines.**
  - SQLite is an ACID relational database with write-ahead logging (WAL).
  - Git is a distributed content-addressable object database and working tree manager.
  - A single atomic transaction spanning both SQLite and Git does NOT exist without a distributed Two-Phase Commit (2PC) coordinator.
- **The Failure Modes:**
  1. If a crash occurs after Git updates `main` but before SQLite updates `missions.state = 'integrated'`, SQLite thinks the mission is still `accepted`, while Git already contains the commit.
  2. If a crash occurs after SQLite updates `missions.state = 'integrating'` but before Git finishes merging, SQLite thinks integration is in progress or done, while Git may be in an aborted, conflicting, or untouched state.
  3. If two concurrent HTTP requests hit `/integrate` on the same workspace, race conditions in Git checkout/merge could corrupt the working tree or produce spurious conflict failures.

### The Truthful Architectural Resolution
Axonel v1 eliminates misleading claims of magical cross-engine ACID. Instead, Axonel guarantees **eventual consistency with zero false reporting** through:
1. **Durable Intent Logging:** Moving to an intermediate state `MissionState::Integrating` with durable `integration_intent` recorded in SQLite before touching Git.
2. **Per-Workspace Concurrency Serialization:** Enforcing strict intra-process serialization via `WorkspaceLockManager`.
3. **Physical Git Disk Authoritativeness on Recovery:** On server startup or explicit reconciliation, the physical Git repository is inspected as the ultimate ground truth (`git merge-base --is-ancestor`). If the candidate commit is reachable in the target branch, state moves to `Integrated`. If not, any dangling merge is cleanly aborted (`git merge --abort`) and state is reset to `Accepted`.
4. **Zero False Reporting Invariant:** Axonel **never** reports a mission as `integrated` if the target Git branch does not physically contain the candidate commit.

---

## 2. The Enhanced v1 Lifecycle

The canonical Axonel v1 lifecycle is now:

$$\text{Created} \to \text{Planning} \to \text{Running} \to \text{Verifying} \to \mathbf{AwaitingAcceptance} \to \mathbf{Accepted} \to \mathbf{Integrating} \to \mathbf{Integrated}$$

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
                        │                         ▼
                        │                   [Integrating] (durable intent logged)
                        │                    │         │
                        │   (git fails/crash)│         ▼ (git succeeds)
                        │                    └────►[Integrated]
                        │                          (target branch contains commit)
                        └───────────────────────────┘
```

### State Transition Rules:
- `Accepted -> Integrating`: Initiated by `POST /integrate` or `POST /accept` with `integrate: true`. Requires verified candidate commit and passing final outcome. Acquires workspace lock. Logs `integration_intent`.
- `Integrating -> Integrated`: Git merge succeeds and target HEAD contains candidate commit. Emits `mission_integration_succeeded`.
- `Integrating -> Accepted`: Git merge fails (conflict, dirty tree, stale branch) OR startup reconciler finds commit was not integrated before crash. Reverts state cleanly and emits `mission_integration_failed` or `mission_integration_reconciled`.
- `Integrating -> Cancelled`: Explicit administrative abort during stalled integration.

---

## 3. Concurrency & Integrity Protections

### Intra-Process Concurrency (`WorkspaceLockManager`)
- All repository mutations across HTTP endpoints, background runners, and CLI commands acquire a per-workspace mutex (`Arc<tokio::sync::Mutex<()>>`).
- Double-check pattern under lock: Re-fetches mission state from the database immediately after acquiring the lock to prevent duplicate integrations from concurrent requests.

### External / Inter-Process Concurrency
- `expected_target_head` validation prevents clobbering commits made by external developers or processes.
- Git native `--ff-only` check prevents accidental recursive merges when simple fast-forwards are expected.

### Review Package Trust Model
- Review Package now exposes:
  - `verified_target_head`: Target branch HEAD SHA at the time verification passed.
  - `current_target_head`: Target branch HEAD SHA currently on disk.
  - `reverification_required`: Boolean flag (`current_target_head != verified_target_head`).
- The Web UI displays a prominent warning banner when target HEAD has drifted, advising the developer before acceptance.

---

## 4. Automated Verification: The 15 M20 Reliability Scenarios

We authored and verified the complete M20 automated test suite (`web/tests/m20_integration_reliability_tests.mjs`). The suite cleanly distinguishes **Domain/Policy Tests** from **Real Product E2E Tests**:

| Scenario | Scope | Description | Result | Evidence |
|---|---|---|---|---|
| **Scenario A** | Policy | Happy-path integration lifecycle | **PASS** | Target HEAD matches candidate commit; ancestry verified on disk. |
| **Scenario B** | Policy | Duplicate integration request idempotency | **PASS** | Returns HTTP 200 with `already_integrated: true`. |
| **Scenario C** | Policy | Concurrent duplicate integration requests | **PASS** | Workspace lock serializes requests; one integrates, other reports already integrated. |
| **Scenario D** | Policy | Merge conflict atomic rollback | **PASS** | Returns HTTP 409 Conflict, executes `git merge --abort`, tree clean, state `accepted`. |
| **Scenario E** | Policy | Dirty target repository rejection | **PASS** | Returns HTTP 409 Conflict, developer uncommitted file preserved intact, state `accepted`. |
| **Scenario F** | Policy | Stale target branch rejection | **PASS** | Detects drift from `expected_target_head`, returns HTTP 409 Conflict without clobbering. |
| **Scenario G** | Policy | Crash during integration (Git incomplete) -> reset to Accepted | **PASS** | Reconciler inspects disk, cleans merge, resets state to `accepted` (`not_integrated_on_disk`). |
| **Scenario H** | Policy | Restart during integration truthful resolution | **PASS** | Server SIGKILL during integration; startup reconciler restores truthful `accepted` state. |
| **Scenario I** | Policy | Git success / DB failure -> Reconciled to Integrated | **PASS** | Candidate commit merged on disk before crash; reconciler promotes state to `integrated`. |
| **Scenario J** | Policy | DB success status truthfulness | **PASS** | Target branch contains commit; state is `integrated`. |
| **Scenario K** | Policy | Reconciliation idempotency & audit | **PASS** | Reconciler runs repeatedly without state churn; reports 0 unassigned. |
| **Scenario L** | Policy | Previously integrated mission preserved across restart | **PASS** | Server SIGKILL on integrated mission; restart cleanly preserves `integrated`. |
| **Scenario M** | Policy | Accepted-but-not-integrated preserved without auto-integrating | **PASS** | Restart preserves `accepted` state; does not bypass human release gate. |
| **Scenario N** | Policy | Rejected mission safety | **PASS** | Rejected mission cannot be integrated; restart preserves `rejected` state. |
| **Scenario O** | **Real E2E** | Real Gemini Full Workflow (Autonomous -> Review -> Accept & Integrate) | **PASS** | Gemini CLI autonomously fixes bug, stopping condition verified on disk, human accepts, `cargo test` passes in target repository. |

**Test Result:** **15 / 15 PASSED (100%)**

---

## 5. Regression Suite Verification

In addition to the M20 reliability suite, all existing test suites were executed and verified:
1. `node web/tests/m19_acceptance_tests.mjs`: **15 / 15 PASSED (100%)**
2. `node web/tests/rc_matrix_tests.mjs`: **15 / 15 PASSED (100%)**
3. `cargo test --workspace`: **All unit, integration, stress, and security tests PASSED**
4. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: **0 warnings**
5. `npm --prefix web run build`: **Vite production build PASSED**

---

## 6. Summary of Commitments & Guarantees

1. **Eventual Consistency without False Claims:** Axonel explicitly documents that Git and SQLite are separate engines, coordinated via durable intent and Git-authoritative recovery.
2. **Zero False Reporting:** Axonel never reports `integrated` unless Git's physical target branch contains the commit.
3. **Pristine Rollback:** Any Git integration failure cleans up all Git state (`git merge --abort`) and returns the mission to `accepted`.
4. **Inviolable Human Governance:** Server crashes, reconciler runs, and CLI operations never bypass human acceptance gates.
