# Axonel Public v0.1.1 Go / No-Go Release Decision Matrix

**Evaluation Date:** September 19, 2026  
**Target Release:** Axonel v0.1.1 (Audit Hardened Patch Release)  
**Evaluator:** Axonel Release Engineering  
**Certified Platform:** Linux x86_64 (`x86_64-unknown-linux-gnu`)  

This document records the definitive, evidence-backed Go / No-Go decision for the public v0.1.1 release of Axonel following the external audit. Every row is evaluated strictly as **PASS**, **FAIL**, or **NOT VERIFIED**.

---

## 1. Release Gate Decision Table

| # | Requirement Area | Release Criterion | Verification Evidence | Status | Blocker? |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 1 | **Network Security** | Safe default bind (`127.0.0.1:3000`) | `crates/plexis-server/tests/security_tests.rs::test_a_default_bind_loopback_auth_status` | **PASS** | None |
| 2 | **Network Security** | Authenticated external bind (fails without token) | `crates/plexis-server/tests/security_tests.rs::test_b_non_loopback_without_auth_fails_startup` | **PASS** | None |
| 3 | **Agent Supervision** | Real Gemini CLI process execution | Empirical run on target repo; process group confinement verified; `web/tests/e2e_release_candidate.mjs` | **PASS** | None |
| 4 | **Verification** | Independent physical verification on disk | Out-of-band `cargo test`, `npm test`, and `pytest` execution; dirty tree invariant checked | **PASS** | None |
| 5 | **Human Governance**| Explicit human acceptance gate | Autonomous halts at `AwaitingAcceptance`; unaccepted integration blocked with HTTP 409 | **PASS** | None |
| 6 | **Git Reliability** | Safe transactional Git integration | Authoritative Git ancestry recovery; atomic rollback on merge conflict (`m20_integration_reliability_tests.mjs`) | **PASS** | None |
| 7 | **Durability** | Crash recovery & state reconciliation | `SIGKILL` termination durably reconciled via `git merge-base --is-ancestor` with zero data loss | **PASS** | None |
| 8 | **Browser Release** | Full browser release flow E2E | Playwright Chromium test `web/tests/e2e_release_candidate.mjs` (15/15 assertions passed) | **PASS** | None |
| 9 | **Installation** | Clean-checkout build from source | Pristine checkout in `/tmp/axonel_clean_install_test` builds web assets and binary | **PASS** | None |
| 10| **Release Binary** | Production release build (`--release`) | Production `target/release/axonel` binary compiled, verified with `axonel --help` and `axonel --version` | **PASS** | None |
| 11| **Continuous Integration** | Remote CI green on `origin/main` | GitHub Actions passed all checks on remote `main` branch | **PASS** | None |
| 12| **Documentation** | Truthful docs without marketing hype | `README.md`, `CHANGELOG.md`, `docs/V1_LIMITATIONS.md`, `docs/REAL_WORLD_VALIDATION_RESULTS.md` | **PASS** | None |
| 13| **Version Identity**| Consistent public version (`0.1.1`) | `Cargo.toml`, `package.json`, and `axonel --version` truthfully output `0.1.1` | **PASS** | None |
| 14| **Release Artifact**| Reproducible artifact strategy & SHA-256 | `docs/RELEASE_ARTIFACTS.md` documents packaging, archive layout, and checksum verification | **PASS** | None |
| 15| **Platform Scope** | Accurate platform support claims | Certified strictly for `Linux x86_64`; unverified architectures (`aarch64`) documented as planned | **PASS** | None |
| 16| **Audit Hardening** | `main` branch protection invariant | Worktree isolation enforced; no direct agent commits on `main` before acceptance (`quickstart_safety_regression_test.rs`) | **PASS** | None |
| 17| **Audit Hardening** | Untracked file hygiene & exclusions | Built-in git staging excludes `.db`, `.sqlite`, and lockfiles; identity is `Axonel Agent` | **PASS** | None |
| 18| **Audit Hardening** | Lease claim concurrency safety | Deterministic `claim_command_by_id` eliminates command lease race flakes under concurrency | **PASS** | None |
| 19| **Audit Hardening** | Container/Root sandbox compatibility | Bubblewrap builder binds `/root` and `$HOME` with fallback; passes under root runners | **PASS** | None |

---

## 2. Final Blocker Audit

- **P0 Blockers (Data loss, auth bypass, unverified integration, false integrated state, leaked secrets, broken install/build/browser, broken CI):**
  - **COUNT: 0**
  - All critical invariants verified through unit, integration, browser, and security suites.
- **P1 Blockers (Misleading docs, broken first-run flow, unsupported platform claims, severe UX confusion):**
  - **COUNT: 0**
  - Unsupported `aarch64` claim removed and documented as planned; first-run experience verified with clean CLI commands; limitations transparently published in `docs/V1_LIMITATIONS.md`.

---

## 3. Go / No-Go Determination

```text
================================================================================
                    FINAL RELEASE DETERMINATION: GO
================================================================================
All 19 release criteria are marked PASS with zero P0 or P1 blockers.
Axonel has satisfied all release validation criteria with concrete evidence for Public v0.1.1 Release.
================================================================================
```
