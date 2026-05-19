# Chappie — One-Command, No-Rust Install (Design)

**Date:** 2026-05-20
**Status:** Approved
**Repository:** https://github.com/Mic-360/chappie

## Goal

After installing the Chappie plugin, the user does **nothing else**. The Rust
audio daemon appears automatically on the next Claude Code session, on Windows,
Linux, and macOS — with or without the Rust toolchain installed.

Today the plugin ships only Rust source: the user must run `cargo build
--release` (or `/chappie:setup`), which hard-requires Rust. This design removes
that manual build step entirely.

## Non-Goals

- Changing the audio engine, rhythm model, or `signal` protocol in `src/main.rs`.
- Publishing to the official Claude Code marketplace (separate effort).
- Windows ARM64 native binaries — Windows ARM transparently emulates x86_64, so
  the `windows-x86_64` asset serves those machines.

## Architecture

Three independent pieces:

1. **Release CI** — builds prebuilt binaries for every supported platform and
   publishes them as GitHub Release assets.
2. **Auto-bootstrap** — on session start, downloads the matching prebuilt binary
   into the plugin's `target/release/` directory if it is not already there.
3. **Docs & skills** — updated to describe the no-build flow; `/chappie:setup`
   becomes a repair tool rather than a required step.

```
push tag v1.2.0
      │
      ▼
.github/workflows/release.yml  ──►  GitHub Release with 5 binary assets
                                          │
                                          │  releases/latest/download/<asset>
                                          ▼
Claude Code SessionStart hook ──► scripts/chappie-bootstrap.{sh,cmd}
                                          │  curl/wget download
                                          ▼
              ${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon[.exe]
                                          │
                                          ▼
        existing signal hooks (UserPromptSubmit, PreToolUse, ...) just work
```

## Component 1 — Release CI

**New file:** `.github/workflows/release.yml`

**Trigger:** push of a tag matching `v*`. Also `workflow_dispatch` for manual
runs.

**Build matrix:**

| Runner | Rust target | Asset name |
|---|---|---|
| `ubuntu-24.04` | `x86_64-unknown-linux-gnu` | `chappie-daemon-linux-x86_64` |
| `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | `chappie-daemon-linux-aarch64` |
| `macos-14` | `aarch64-apple-darwin` | `chappie-daemon-macos-aarch64` |
| `macos-14` | `x86_64-apple-darwin` | `chappie-daemon-macos-x86_64` |
| `windows-latest` | `x86_64-pc-windows-msvc` | `chappie-daemon-windows-x86_64.exe` |

Native ARM runners are used so ALSA/CoreAudio link natively — no cross-compiling.
The two macOS targets build on one `macos-14` runner (the native toolchain
supports `x86_64-apple-darwin` via `rustup target add`).

**Per-job steps:**

1. Checkout.
2. Install Rust via `dtolnay/rust-toolchain@stable`; add the job's target.
3. Linux jobs: `sudo apt-get install -y libasound2-dev pkg-config` (rodio/ALSA).
4. `cargo build --release --target <target>`.
5. Rename `target/<target>/release/chappie-daemon[.exe]` to the asset name.
6. Upload as a build artifact.

**Publish job** (`needs:` all build jobs): download all artifacts, attach them to
the GitHub Release for the tag using `softprops/action-gh-release@v2`. The job
needs `permissions: contents: write`.

## Component 2 — Auto-bootstrap

### Hook wiring

**File:** `hooks/hooks.json` — add a `SessionStart` event with **two** hook
entries; existing `signal` hooks are unchanged.

```json
"SessionStart": [
  {
    "matcher": "*",
    "hooks": [
      { "type": "command",
        "command": "sh \"${CLAUDE_PLUGIN_ROOT}/scripts/chappie-bootstrap.sh\"",
        "timeout": 60 },
      { "type": "command",
        "command": "\"${CLAUDE_PLUGIN_ROOT}/scripts/chappie-bootstrap.cmd\"",
        "timeout": 60 }
    ]
  }
]
```

Cross-platform mechanism: on Linux/macOS the `.sh` entry runs and the `.cmd`
entry fails harmlessly (`cmd` script is not executable / interpreter absent); on
Windows the `.cmd` entry runs and the `sh ...` entry fails harmlessly (`sh` not
found). Hook failures are non-fatal in Claude Code, so the wrong-OS entry is a
silent no-op. `SessionStart` runs before any user prompt, so the binary is in
place before the first `signal` hook fires.

### `scripts/chappie-bootstrap.sh` (POSIX sh — Linux & macOS)

Behavior:

1. Resolve `PLUGIN_ROOT` as the parent of the script's own directory
   (`dirname "$0"`), so all paths are **relative to the install location**.
2. Set `BIN="$PLUGIN_ROOT/target/release/chappie-daemon"`.
3. **Idempotent exit:** if `BIN` exists and is executable, exit 0 immediately.
4. Detect platform with `uname -s` (`Linux`/`Darwin`) and `uname -m`
   (`x86_64`/`amd64` → `x86_64`; `aarch64`/`arm64` → `aarch64`). Map to the
   asset name. Unknown combination → log and exit 0 (never block the session).
5. Download `https://github.com/Mic-360/chappie/releases/latest/download/<asset>`
   to a temp file via `curl -fsSL --retry 2` (fallback `wget -q`).
6. On success: `chmod +x` the temp file; on macOS run
   `xattr -d com.apple.quarantine` (ignore errors) so Gatekeeper does not block
   the unsigned binary; atomically `mv` it to `BIN`.
7. **Fallback:** if download fails and `cargo` is on `PATH`, run
   `cargo build --release` in `PLUGIN_ROOT`.
8. If everything fails, write a clear message to the log and exit 0.
9. All progress/errors are appended to
   `~/.claude/.chappie_state/bootstrap.log` (`HOME`-based; created if needed).

### `scripts/chappie-bootstrap.cmd` (Windows batch)

Mirror of the `.sh` script:

1. `PLUGIN_ROOT` derived from `%~dp0` (script directory) — relative paths.
2. `BIN=%PLUGIN_ROOT%\target\release\chappie-daemon.exe`.
3. Idempotent exit if `BIN` exists.
4. `%PROCESSOR_ARCHITECTURE%` / `%PROCESSOR_ARCHITEW6432%` → only
   `chappie-daemon-windows-x86_64.exe` is published; ARM64 Windows uses it via
   emulation.
5. `curl -fsSL --retry 2` download to a temp file, then `move` into place
   (create `target\release` with `mkdir` first).
6. Fallback to `cargo build --release` if `cargo` is available.
7. Log to `%USERPROFILE%\.claude\.chappie_state\bootstrap.log`.
8. Always exit 0 so a failure never blocks the session.

### Version policy

The bootstrap pulls `releases/latest/download/...`. The plugin and the daemon
share the `signal` protocol, which is stable, so "latest binary" is safe across
plugin patch versions. `RELEASING.md` documents keeping the Release tag in step
with `plugin.json`'s `version`.

## Component 3 — Docs & Skills

### `README.md`

- Rewrite **Installation**: two standard commands to install
  (`/plugin marketplace add Mic-360/chappie`, then
  `/plugin install chappie@chappie-marketplace`), then **zero** manual setup —
  the daemon auto-installs on the next session. State plainly that Rust is *not*
  required.
- Remove the inaccurate "No shell scripts are involved" statement; replace with
  an accurate description of the paired-bootstrap mechanism.
- Update **Plugin Structure** tree to include `.github/`, `scripts/`, `docs/`.
- Fix all `mic-360` references to `Mic-360`.
- Keep the manual `cargo build` instructions under a "Build from source"
  subsection for contributors.

### `skills/setup/SKILL.md`

Repurpose `/chappie:setup` as a **repair / reinstall** command: force re-download
of the prebuilt binary, optionally build from source, run diagnostics. No longer
described as a required post-install step.

### `skills/status/SKILL.md`

Add a check for `bootstrap.log` and report whether the binary was obtained via
download or local build.

### `docs/RELEASING.md` (new)

Maintainer guide: bump `version` in `Cargo.toml`, `plugin.json`,
`marketplace.json`; commit; `git tag vX.Y.Z`; `git push --tags`; the workflow
builds and publishes. How to verify assets and re-run a failed build.

### Metadata

Fix repository owner casing (`mic-360` → `Mic-360`) in `plugin.json` and
`marketplace.json`.

## Files Changed

| File | Change |
|---|---|
| `.github/workflows/release.yml` | new — release build matrix |
| `scripts/chappie-bootstrap.sh` | new — POSIX bootstrap |
| `scripts/chappie-bootstrap.cmd` | new — Windows bootstrap |
| `hooks/hooks.json` | add paired `SessionStart` hooks |
| `skills/setup/SKILL.md` | repurpose as repair command |
| `skills/status/SKILL.md` | report bootstrap state |
| `README.md` | rewrite install flow; fix casing; structure tree |
| `docs/RELEASING.md` | new — maintainer release guide |
| `.claude-plugin/plugin.json` | fix owner casing |
| `.claude-plugin/marketplace.json` | fix owner casing |

`src/main.rs` is **not** modified.

## Edge Cases & Decisions

- **macOS Gatekeeper:** downloaded unsigned binaries are quarantined and refused.
  The bootstrap strips the `com.apple.quarantine` attribute after download.
- **Offline / download failure:** bootstrap falls back to `cargo build` when Rust
  is present; otherwise logs a clear message and exits 0. The session is never
  blocked — Chappie is silent until a later session succeeds.
- **Idempotency:** the bootstrap is a per-session hook; it must exit in
  milliseconds when the binary already exists.
- **Relative paths:** scripts resolve every path from their own location
  (`dirname "$0"` / `%~dp0`), never from a hardcoded or absolute path, so the
  plugin works regardless of where Claude Code installs it.
- **Wrong-OS hook entry:** relies on Claude Code treating a non-zero hook exit as
  non-fatal. This is current Claude Code behavior; the timeout (60s) is generous
  for the download.
- **No release published yet:** until the maintainer pushes the first `v*` tag,
  `releases/latest/download/...` 404s and the bootstrap falls back to source
  build. The first release tag must be cut as part of rollout.

## Testing / Verification

- `cargo build --release` and `cargo test` still pass (no `src/` change).
- `scripts/chappie-bootstrap.sh` run manually on Linux and macOS: downloads the
  binary, second run is a fast no-op.
- `scripts/chappie-bootstrap.cmd` run manually on Windows: same.
- `release.yml` validated via `workflow_dispatch` on a test tag; confirm all five
  assets attach to the Release.
- End-to-end: install the plugin on a Rust-free machine, start a session, confirm
  typing sounds play with no manual step.
