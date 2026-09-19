# Axonel Reproducible Release Guide

**Version:** 1.0  
**Target:** Axonel v0.1.0 Public Release

This document provides step-by-step instructions to compile, verify, and package Axonel deterministically from source on any supported Linux or macOS system.

---

## 1. System Prerequisites

Ensure the following build tools are installed:

| Tool | Minimum Version | Recommended | Notes |
| :--- | :--- | :--- | :--- |
| **Rust / Cargo** | `1.80.0` | Stable (`1.80+`) | Installed via `rustup` |
| **Node.js** | `v18.0.0` | `v20.x LTS` | Required for web UI build |
| **npm** | `9.0.0` | `10.x` | Package manager for UI |
| **Git** | `2.34.0` | `2.40+` | Must support `git worktree` |
| **SQLite3** | Bundled | Bundled (`rusqlite`) | Handled automatically by Cargo |

---

## 2. Source Compilation

### Step 1: Clone Repository
```bash
git clone https://github.com/axonel/axonel.git
cd axonel
```

### Step 2: Build Web Dashboard Assets
```bash
cd web
npm ci
npm run build
cd ..
```
*Verification:* Ensure `web/dist/index.html` exists and is approximately 1.1 KB.

### Step 3: Compile Rust Release Binaries
```bash
cargo build --release --workspace
```
This produces three release binaries in `target/release/`:
- `axonel`: The primary CLI and control plane binary.
- `plexis`: Backward-compatible binary alias.
- `plexis-server`: Control plane daemon server binary.

---

## 3. Release Quality Gate Verification

Before publishing or deploying any release build, execute the canonical quality gate suite:

### 1. Code Formatting Check
```bash
cargo fmt --all -- --check
```
*Expected output: Exit code 0, no diff.*

### 2. Clippy Linter Check
```bash
cargo clippy --workspace --all-targets -- -D warnings
```
*Expected output: Zero warnings, exit code 0.*

### 3. Rust Unit & Integration Test Suite
```bash
cargo test --workspace
```
*Expected output: All unit, storage, runtime, and security tests pass.*

### 4. Security Defaults Test Suite
```bash
cargo test --test security_tests
```
*Expected output: 6 passed (Tests A through F).*

### 5. Deterministic Regression Suites
Build test dependencies:
```bash
cargo build -p plexis-server --bin axonel --bin plexis-server
cargo build -p plexis-fake-agent
```

Run M19 Human Acceptance Suite:
```bash
node web/tests/m19_acceptance_tests.mjs
```
*Expected output: 100% passed across all acceptance scenarios.*

Run M20 Integration Reliability Suite:
```bash
node web/tests/m20_integration_reliability_tests.mjs
```
*Expected output: 100% passed across all transactional integration scenarios.*

---

## 4. Verification Checksums

Generate SHA-256 checksums for release binaries:

```bash
sha256sum target/release/axonel > axonel-checksums.txt
sha256sum target/release/plexis-server >> axonel-checksums.txt
cat axonel-checksums.txt
```

---

## 5. Local Smoke Test

Verify that the compiled binary boots with hardened security defaults:

```bash
# 1. Default loopback startup (should succeed)
./target/release/axonel serve --port 3000 &
SERVER_PID=$!
sleep 2

# 2. Check health and auth status
curl -s http://127.0.0.1:3000/health
curl -s http://127.0.0.1:3000/api/v1/auth/status

# 3. Clean shutdown
kill $SERVER_PID

# 4. Verify non-loopback without auth fails hard (should fail with exit code 1)
./target/release/axonel serve --host 0.0.0.0 --port 3000
echo "Exit code: $?" # Must be non-zero
```
