# Axonel Real-World Validation Framework

**Version:** 1.0  
**Status:** Evaluation Standard & Dataset Schema  

This document specifies the experimental methodology, dataset schema, and evaluation protocol used to validate Axonel on real-world software engineering repositories.

---

## 1. Objectives & Scope

The goal of this framework is to establish an objective, reproducible benchmark for autonomous engineering agents operating under the Axonel supervisor.

The evaluation focuses exclusively on **falsifiable, binary-verifiable engineering tasks**:
1. **Tech Debt & Major Version Refactoring:** Migrating deprecated APIs or framework versions (e.g. `axum 0.6 -> 0.7`, `tokio` migrations).
2. **Flaky Test Remediation:** Isolating and resolving timing races, unseeded random tests, or async deadlocks.
3. **Breaking Dependency Upgrades:** Bumping major versions in `Cargo.toml` or `package.json` and fixing compilation errors.
4. **Reproducible Bug Fixes:** Resolving defects that have failing unit or integration tests.

---

## 2. Dataset Schema

Every benchmark task is defined as a self-contained JSON document:

```json
{
  "$schema": "https://axonel.dev/schemas/v1/validation-task.json",
  "task_id": "axonel-val-2026-001",
  "repository": {
    "url": "https://github.com/example/sample-rust-service",
    "base_commit": "a1b2c3d4e5f67890123456789abcdef01234567",
    "language": "rust",
    "build_system": "cargo"
  },
  "category": "tech_debt",
  "title": "Migrate HTTP handlers to Axum 0.8 routing style",
  "objective": "Update all router declarations and state extractors in src/routes.rs to match Axum 0.8 conventions. Ensure 'cargo test' passes with 0 errors.",
  "stopping_condition": {
    "required_tests_pass": true,
    "working_tree_clean": true,
    "required_commit_exists": true,
    "custom_command": "cargo clippy -- -D warnings"
  },
  "budget": {
    "max_duration_secs": 1800,
    "max_executions": 10,
    "max_stagnant_cycles": 3
  },
  "evaluation": {
    "ground_truth_test_command": "cargo test --all-targets",
    "expected_files_modified": [
      "Cargo.toml",
      "src/routes.rs"
    ]
  }
}
```

---

## 3. Standard Evaluation Protocol

Each evaluation trial follows a strict 6-stage protocol:

```text
  [1. Ingest Task] ────► [2. Worktree Spawn] ────► [3. Agent Cycle]
                                                         │
  [6. Human Review] ◄─── [5. Disk Verification] ◄── [4. Replanning]
         │
         ▼
  [7. Git Integration]
```

1. **Ingest Task:** The test runner clones the target repository and checks out `base_commit`.
2. **Worktree Spawn:** Axonel creates an isolated worktree (`.plexis/worktrees/<task_id>`).
3. **Agent Cycle:** The external agent (e.g. Gemini CLI) executes autonomously in the worktree.
4. **Replanning (if needed):** If an execution stalls or produces zero diff, the supervisor daemon triggers replanning up to `max_stagnant_cycles`.
5. **Disk Verification:** The out-of-band verifier executes `ground_truth_test_command` on disk.
6. **Human Review:** The review package is presented to an independent human evaluator (or simulated operator in automated benchmarks).
7. **Git Integration:** Upon acceptance, the commit is merged into `main` using the canonical integration engine.

---

## 4. Key Performance Indicators (KPIs)

| Metric | Definition | Benchmark Target |
| :--- | :--- | :--- |
| **First-Pass Verification Rate (FPVR)** | Percentage of tasks verified on Cycle 0 without replanning | $\ge 60\%$ |
| **Replanning Recovery Rate (RRR)** | Percentage of failed Cycle 0 tasks successfully recovered via replanning | $\ge 40\%$ |
| **Total Task Resolution Rate (TTRR)** | Overall percentage of tasks reaching `AwaitingAcceptance` | $\ge 80\%$ |
| **Clean Integration Rate (CIR)** | Percentage of accepted tasks merged into target branch without merge conflict | $\ge 95\%$ |
| **Stagnation Detection Time** | Time taken by supervisor to detect an inactive/looping agent | $\le 120\text{s}$ |
| **Crash Recovery Completeness** | Percentage of missions correctly reconciled after simulated SIGKILL | $100\%$ |

---

## 5. Release Evidence

The validation framework was executed end-to-end against real repositories with the following observed facts:

| Benchmark Dimension | Empirical Result | Notes |
|---|---|---|
| **Real-World 20-Task Dataset** | **Published & Structured** | [`docs/validation/dataset.json`](./validation/dataset.json): 20 tasks across Rust, TS, and Python. |
| **Real-World Baseline Evaluation**| **Completed & Documented**| [`docs/REAL_WORLD_VALIDATION_RESULTS.md`](./REAL_WORLD_VALIDATION_RESULTS.md): Direct Gemini vs Axonel baseline compared. |
| **Operational Value Proven** | **8 Responsibilities Removed** | Terminal monitoring, out-of-band testing, failure retries, git cleanliness, and merge reconciliation. |
| **Real Browser E2E** | **15 / 15 PASSED (100%)** | `web/tests/e2e_release_candidate.mjs`: Complete user lifecycle verified via Playwright Chromium. |
| **Real Agent Execution** | **PASS (Proven)** | Google Gemini CLI v0.60.0 repaired Rust code defect, achieved `cargo test` pass on disk, committed cleanly. |
| **CI Remote Status** | **100% Green (Run 35433550561)**| GitHub Actions `.github/workflows/ci.yml` executed on push to `main`. |
| **Crash Recovery** | **100% (15 / 15)** | `m20_integration_reliability_tests.mjs`: Zero false reports, Git ancestry authoritative reconciliation verified. |
| **Human Acceptance Gate** | **100% (15 / 15)** | `m19_acceptance_tests.mjs`: Unaccepted integration strictly blocked; review package rendered; non-destructive rejection verified. |
| **Quality Gates** | **PASS (0 warnings)** | `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, and web production build clean. |
| **Known Limitations** | **Documented** | Live external agent execution requires valid local credentials; Claude/Codex/OpenCode adapters are stubs. |
