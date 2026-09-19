# Axonel Release Artifact Specification

**Version:** 0.1.0  
**Target Release:** Axonel v0.1.0 (Public Release)  
**Host Architecture:** Linux x86_64 (`x86_64-unknown-linux-gnu`)  
**Date:** September 19, 2026  

This document specifies the reproducible release artifact packaging, distribution layout, and cryptographic verification strategy for Axonel v0.1.0.

---

## 1. Supported Platform Scope

| Platform / Architecture | Target Triple | Support Status | Verification Basis |
| :--- | :--- | :--- | :--- |
| **Linux (x86_64)** | `x86_64-unknown-linux-gnu` | **Certified Release** | Fully build-tested, quality-gated, and empirically verified in Linux environment. |
| **Linux (aarch64)** | `aarch64-unknown-linux-gnu` | **Planned / Unverified** | Theoretically compatible with Rust standard library, but not natively build-tested on actual ARM64 hardware in this release. |
| **macOS (Apple Silicon)** | `aarch64-apple-darwin` | **Planned / Experimental** | POSIX signal handling and worktrees supported; Darwin sandbox integration planned for future release. |
| **Windows** | `x86_64-pc-windows-msvc` | **Unsupported** | Linux/POSIX process tree (`killpg`, `setpgid`, `SIGTERM`/`SIGKILL`) semantics required. |

> [!IMPORTANT]
> In accordance with our evidence integrity standard, Axonel does NOT publish or claim certified support for architectures that have not been physically built and tested. For v0.1.0, precompiled binaries and release verification are certified strictly on **Linux x86_64**.

---

## 2. Release Deliverables

For each public GitHub release, Axonel produces four primary deliverables:

```text
release-artifacts/
├── axonel-v0.1.0-x86_64-unknown-linux-gnu.tar.gz  # Compiled CLI & control plane binary + docs
├── axonel-v0.1.0-source.tar.gz                     # Canonical source archive (git archive)
├── axonel-web-v0.1.0.tar.gz                        # Pre-built Vite SPA static dashboard assets
└── SHA256SUMS.txt                                  # Cryptographic SHA-256 checksums
```

### Artifact Details:

1. **`axonel-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`**:
   - Contains:
     - `bin/axonel`: Stripped production release binary (`cargo build --release -p plexis-server --bin axonel`).
     - `web/dist/`: Embedded or bundled production web assets.
     - `README.md`, `LICENSE`, `docs/V1_LIMITATIONS.md`.
2. **`axonel-v0.1.0-source.tar.gz`**:
   - Clean source archive created directly from Git:
     ```bash
     git archive --format=tar.gz --prefix=axonel-0.1.0/ -o axonel-v0.1.0-source.tar.gz HEAD
     ```
3. **`axonel-web-v0.1.0.tar.gz`**:
   - Standalone web client bundle containing `web/dist/` (`index.html`, JavaScript chunks, CSS stylesheets, and icon assets) for static hosting or decoupled deployment.
4. **`SHA256SUMS.txt`**:
   - Cryptographic SHA-256 hash manifest covering all release archives.

---

## 3. Reproducible Packaging Procedure

To generate the certified release artifacts on a Linux x86_64 host:

```bash
# 1. Ensure clean git status
git checkout v0.1.0
git status --porcelain # Must be empty

# 2. Build Web Dashboard
npm --prefix web ci
npm --prefix web run build

# 3. Build Release Binary
cargo build --release -p plexis-server --bin axonel
strip target/release/axonel

# 4. Assemble Artifact Directory
mkdir -p dist/bin dist/web
cp target/release/axonel dist/bin/
cp -r web/dist/* dist/web/
cp README.md LICENSE docs/V1_LIMITATIONS.md dist/

# 5. Archive Binary Distribution
tar -czf axonel-v0.1.0-x86_64-unknown-linux-gnu.tar.gz -C dist .

# 6. Archive Source Distribution
git archive --format=tar.gz --prefix=axonel-0.1.0/ -o axonel-v0.1.0-source.tar.gz HEAD

# 7. Archive Web Assets
tar -czf axonel-web-v0.1.0.tar.gz -C web/dist .

# 8. Generate Checksums
sha256sum axonel-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
          axonel-v0.1.0-source.tar.gz \
          axonel-web-v0.1.0.tar.gz > SHA256SUMS.txt

# 9. Verify Checksums
sha256sum -c SHA256SUMS.txt
```

---

## 4. End-User Installation & Verification

End users can verify and install the release package as follows:

```bash
# 1. Download binary archive and checksums
curl -LO https://github.com/axonel/axonel/releases/download/v0.1.0/axonel-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/axonel/axonel/releases/download/v0.1.0/SHA256SUMS.txt

# 2. Verify SHA-256 checksum
sha256sum --ignore-missing -c SHA256SUMS.txt

# 3. Extract to /usr/local/bin
sudo tar -xzf axonel-v0.1.0-x86_64-unknown-linux-gnu.tar.gz -C /usr/local/bin bin/axonel --strip-components=1

# 4. Verify installation
axonel --version
# Output: axonel 0.1.0

axonel serve --help
```
