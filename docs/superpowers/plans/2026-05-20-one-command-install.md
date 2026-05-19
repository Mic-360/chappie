# Chappie One-Command Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Chappie plugin install and run with no manual build step and no Rust toolchain, on Windows, Linux, and macOS.

**Architecture:** A GitHub Actions workflow cross-builds the `chappie-daemon` binary for five platform targets and publishes them as GitHub Release assets. A pair of OS-specific bootstrap scripts, wired to the Claude Code `SessionStart` hook, download the matching prebuilt binary into the plugin's `target/release/` directory on first session — falling back to `cargo build` only if a download fails and Rust is present.

**Tech Stack:** Rust (unchanged), GitHub Actions, POSIX `sh`, Windows batch (`cmd`), Claude Code plugin hooks.

**Spec:** `docs/superpowers/specs/2026-05-20-one-command-install-design.md`

---

## File Structure

| File | Responsibility |
|---|---|
| `.github/workflows/release.yml` (new) | Cross-build all 5 targets on tag push; publish Release assets |
| `scripts/chappie-bootstrap.sh` (new) | POSIX bootstrap: detect OS/arch, download binary, fall back to build |
| `scripts/chappie-bootstrap.cmd` (new) | Windows bootstrap: same, for `cmd` |
| `hooks/hooks.json` (modify) | Add paired `SessionStart` hook entries |
| `.claude-plugin/plugin.json` (modify) | Fix `mic-360` → `Mic-360` casing |
| `.claude-plugin/marketplace.json` (modify) | Fix `mic-360` → `Mic-360` casing |
| `skills/setup/SKILL.md` (modify) | Repurpose as repair/reinstall command |
| `skills/status/SKILL.md` (modify) | Report bootstrap state |
| `README.md` (modify) | Rewrite install flow; fix casing; update structure tree |
| `docs/RELEASING.md` (new) | Maintainer release guide |

`src/main.rs` is **not** modified. There is no automated test suite for shell
scripts or CI YAML — tasks below use manual verification commands instead.

---

### Task 1: Fix repository owner casing

**Files:**
- Modify: `.claude-plugin/plugin.json`
- Modify: `.claude-plugin/marketplace.json`

- [ ] **Step 1: Update `plugin.json`**

In `.claude-plugin/plugin.json`, change the two URLs that contain `mic-360`:

```json
  "author": {
    "name": "Bhaumic",
    "url": "https://github.com/Mic-360"
  },
  "homepage": "https://github.com/Mic-360/chappie",
  "repository": "https://github.com/Mic-360/chappie",
```

- [ ] **Step 2: Update `marketplace.json`**

In `.claude-plugin/marketplace.json`, change the `repo` and the two URLs:

```json
      "source": {
        "source": "github",
        "repo": "Mic-360/chappie"
      },
```

and

```json
      "homepage": "https://Mic-360.github.io/chappie",
      "repository": "https://github.com/Mic-360/chappie",
```

- [ ] **Step 3: Verify no lowercase references remain**

Run: `grep -rn "mic-360" .claude-plugin/`
Expected: no output (exit code 1).

- [ ] **Step 4: Commit**

```bash
git add .claude-plugin/plugin.json .claude-plugin/marketplace.json
git commit -m "fix: correct repository owner casing to Mic-360"
```

---

### Task 2: Add the release build workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create the workflow file**

Create `.github/workflows/release.yml` with exactly this content:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

permissions:
  contents: write

jobs:
  build:
    name: build ${{ matrix.asset }}
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - runner: ubuntu-24.04
            target: x86_64-unknown-linux-gnu
            asset: chappie-daemon-linux-x86_64
            ext: ''
          - runner: ubuntu-24.04-arm
            target: aarch64-unknown-linux-gnu
            asset: chappie-daemon-linux-aarch64
            ext: ''
          - runner: macos-14
            target: aarch64-apple-darwin
            asset: chappie-daemon-macos-aarch64
            ext: ''
          - runner: macos-14
            target: x86_64-apple-darwin
            asset: chappie-daemon-macos-x86_64
            ext: ''
          - runner: windows-latest
            target: x86_64-pc-windows-msvc
            asset: chappie-daemon-windows-x86_64.exe
            ext: '.exe'
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install ALSA (Linux only)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libasound2-dev pkg-config

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Stage binary
        shell: bash
        run: |
          mkdir -p dist
          cp "target/${{ matrix.target }}/release/chappie-daemon${{ matrix.ext }}" "dist/${{ matrix.asset }}"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.asset }}
          path: dist/${{ matrix.asset }}

  release:
    name: publish release
    needs: build
    runs-on: ubuntu-24.04
    if: startsWith(github.ref, 'refs/tags/')
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Publish to GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: artifacts/*
          fail_on_unmatched_files: true
```

- [ ] **Step 2: Validate YAML syntax**

Run: `python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml')); print('valid')"`
Expected: `valid`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow for prebuilt cross-platform binaries"
```

---

### Task 3: Add the POSIX bootstrap script

**Files:**
- Create: `scripts/chappie-bootstrap.sh`

- [ ] **Step 1: Create the script**

Create `scripts/chappie-bootstrap.sh` with exactly this content:

```sh
#!/bin/sh
# Chappie auto-bootstrap (POSIX sh) - Linux & macOS.
# Wired to the Claude Code SessionStart hook. Downloads the prebuilt
# chappie-daemon binary on first session. Idempotent and never fatal:
# always exits 0 so a failure cannot block the session.
set -u

REPO="Mic-360/chappie"

# Resolve all paths relative to this script's own location so the plugin
# works wherever Claude Code installed it.
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PLUGIN_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
BIN_DIR="$PLUGIN_ROOT/target/release"
BIN="$BIN_DIR/chappie-daemon"

LOG_DIR="${HOME:-/tmp}/.claude/.chappie_state"
mkdir -p "$LOG_DIR" 2>/dev/null || true
LOG="$LOG_DIR/bootstrap.log"
log() {
  echo "[chappie-bootstrap] $(date '+%Y-%m-%d %H:%M:%S') $*" >> "$LOG" 2>/dev/null || true
}

# Idempotent: already installed -> exit fast.
if [ -x "$BIN" ]; then
  exit 0
fi

OS=$(uname -s 2>/dev/null || echo unknown)
ARCH=$(uname -m 2>/dev/null || echo unknown)

case "$OS" in
  Linux)  os=linux ;;
  Darwin) os=macos ;;
  *) log "unsupported OS: $OS"; exit 0 ;;
esac

case "$ARCH" in
  x86_64|amd64)  arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) log "unsupported arch: $ARCH"; exit 0 ;;
esac

ASSET="chappie-daemon-${os}-${arch}"
URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

mkdir -p "$BIN_DIR" 2>/dev/null || true
TMP="$BIN_DIR/.chappie-daemon.download"
rm -f "$TMP"

log "downloading $URL"
ok=0
if curl -fsSL --retry 2 --max-time 120 -o "$TMP" "$URL" 2>>"$LOG"; then
  ok=1
elif command -v wget >/dev/null 2>&1 \
  && wget -q --tries=2 --timeout=120 -O "$TMP" "$URL" 2>>"$LOG"; then
  ok=1
fi

if [ "$ok" = 1 ] && [ -s "$TMP" ]; then
  chmod +x "$TMP" 2>/dev/null || true
  if [ "$os" = macos ]; then
    # Strip the quarantine attribute so Gatekeeper does not block the
    # unsigned binary.
    xattr -d com.apple.quarantine "$TMP" 2>/dev/null || true
  fi
  mv -f "$TMP" "$BIN"
  log "installed $BIN"
  exit 0
fi

rm -f "$TMP"
log "download failed"

# Fallback: build from source if Rust is available.
if command -v cargo >/dev/null 2>&1; then
  log "falling back to cargo build"
  if (cd "$PLUGIN_ROOT" && cargo build --release >>"$LOG" 2>&1); then
    log "built from source"
    exit 0
  fi
  log "cargo build failed"
fi

log "could not obtain chappie-daemon; Chappie will be silent this session"
exit 0
```

- [ ] **Step 2: Mark the script executable and record it in git**

```bash
chmod +x scripts/chappie-bootstrap.sh
git update-index --chmod=+x scripts/chappie-bootstrap.sh 2>/dev/null || true
```

- [ ] **Step 3: Syntax-check the script**

Run: `sh -n scripts/chappie-bootstrap.sh && echo "syntax ok"`
Expected: `syntax ok`

- [ ] **Step 4: Run the script and confirm it produces a binary or logs cleanly**

Run: `sh scripts/chappie-bootstrap.sh; echo "exit=$?"; cat ~/.claude/.chappie_state/bootstrap.log`
Expected: `exit=0`. The log shows either `installed .../chappie-daemon`,
`built from source`, or a `download failed` / `could not obtain` line — all
acceptable depending on whether a release exists yet. If a binary was produced,
`ls target/release/chappie-daemon` succeeds.

- [ ] **Step 5: Run it again to confirm idempotency (only if step 4 produced a binary)**

Run: `time sh scripts/chappie-bootstrap.sh; echo "exit=$?"`
Expected: `exit=0`, completes in well under 1 second (fast-path exit). Skip this
step if step 4 did not produce a binary.

- [ ] **Step 6: Commit**

```bash
git add scripts/chappie-bootstrap.sh
git commit -m "feat: add POSIX bootstrap script for prebuilt daemon download"
```

---

### Task 4: Add the Windows bootstrap script

**Files:**
- Create: `scripts/chappie-bootstrap.cmd`

- [ ] **Step 1: Create the script**

Create `scripts/chappie-bootstrap.cmd` with exactly this content:

```bat
@echo off
REM Chappie auto-bootstrap (Windows cmd).
REM Wired to the Claude Code SessionStart hook. Downloads the prebuilt
REM chappie-daemon.exe on first session. Idempotent and never fatal:
REM always exits 0 so a failure cannot block the session.
setlocal EnableExtensions

set "REPO=Mic-360/chappie"

REM Resolve all paths relative to this script's own location.
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "PLUGIN_ROOT=%%~fI"
set "BIN_DIR=%PLUGIN_ROOT%\target\release"
set "BIN=%BIN_DIR%\chappie-daemon.exe"

set "LOG_DIR=%USERPROFILE%\.claude\.chappie_state"
if not exist "%LOG_DIR%" mkdir "%LOG_DIR%" >nul 2>&1
set "LOG=%LOG_DIR%\bootstrap.log"

REM Idempotent: already installed -> exit fast.
if exist "%BIN%" exit /b 0

REM Windows ARM64 transparently emulates x86_64, so one asset serves all.
set "ASSET=chappie-daemon-windows-x86_64.exe"
set "URL=https://github.com/%REPO%/releases/latest/download/%ASSET%"

if not exist "%BIN_DIR%" mkdir "%BIN_DIR%" >nul 2>&1
set "TMP=%BIN_DIR%\.chappie-daemon.download"
if exist "%TMP%" del /f /q "%TMP%" >nul 2>&1

echo [chappie-bootstrap] downloading %URL% >> "%LOG%"
curl -fsSL --retry 2 --max-time 120 -o "%TMP%" "%URL%" >> "%LOG%" 2>&1

set "DLOK="
if exist "%TMP%" for %%A in ("%TMP%") do if %%~zA GTR 0 set "DLOK=1"

if defined DLOK (
  move /y "%TMP%" "%BIN%" >nul 2>&1
  echo [chappie-bootstrap] installed %BIN% >> "%LOG%"
  exit /b 0
)

if exist "%TMP%" del /f /q "%TMP%" >nul 2>&1
echo [chappie-bootstrap] download failed >> "%LOG%"

REM Fallback: build from source if Rust is available.
where cargo >nul 2>&1
if %ERRORLEVEL%==0 (
  echo [chappie-bootstrap] falling back to cargo build >> "%LOG%"
  pushd "%PLUGIN_ROOT%"
  cargo build --release >> "%LOG%" 2>&1
  popd
  if exist "%BIN%" (
    echo [chappie-bootstrap] built from source >> "%LOG%"
    exit /b 0
  )
)

echo [chappie-bootstrap] could not obtain chappie-daemon >> "%LOG%"
exit /b 0
```

- [ ] **Step 2: Run the script on Windows and confirm clean exit**

Run (PowerShell): `cmd /c scripts\chappie-bootstrap.cmd; echo "exit=$LASTEXITCODE"; Get-Content $env:USERPROFILE\.claude\.chappie_state\bootstrap.log -Tail 5`
Expected: `exit=0`. The log tail shows an `installed`, `built from source`,
`download failed`, or `could not obtain` line. If a binary was produced,
`target\release\chappie-daemon.exe` exists.

> If no Windows machine is available to the implementer, record this step as
> "verified by inspection" — the script logic mirrors the verified `.sh` script.

- [ ] **Step 3: Commit**

```bash
git add scripts/chappie-bootstrap.cmd
git commit -m "feat: add Windows bootstrap script for prebuilt daemon download"
```

---

### Task 5: Wire the SessionStart hooks

**Files:**
- Modify: `hooks/hooks.json`

- [ ] **Step 1: Add the `SessionStart` event**

In `hooks/hooks.json`, add a `SessionStart` key as the first entry inside the
`"hooks"` object (before `UserPromptSubmit`). The opening of the file becomes:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "sh \"${CLAUDE_PLUGIN_ROOT}/scripts/chappie-bootstrap.sh\"",
            "timeout": 60
          },
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/scripts/chappie-bootstrap.cmd\"",
            "timeout": 60
          }
        ]
      }
    ],
    "UserPromptSubmit": [
```

Leave every existing event (`UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
`Notification`, `Stop`, `SessionEnd`) exactly as it is.

- [ ] **Step 2: Validate JSON syntax**

Run: `python -c "import json; json.load(open('hooks/hooks.json')); print('valid')"`
Expected: `valid`

- [ ] **Step 3: Confirm the SessionStart block is present and well-formed**

Run: `python -c "import json; h=json.load(open('hooks/hooks.json')); print(len(h['hooks']['SessionStart'][0]['hooks']), 'bootstrap hooks')"`
Expected: `2 bootstrap hooks`

- [ ] **Step 4: Commit**

```bash
git add hooks/hooks.json
git commit -m "feat: auto-bootstrap the daemon on SessionStart"
```

---

### Task 6: Repurpose the setup skill as a repair command

**Files:**
- Modify: `skills/setup/SKILL.md`

- [ ] **Step 1: Replace the file content**

Replace the entire content of `skills/setup/SKILL.md` with:

```markdown
---
name: setup
description: Repair or reinstall the Chappie audio daemon. Use this if Chappie is silent, the daemon binary is missing, or you want to force a fresh download or a build from source.
---

# Chappie Setup / Repair

Chappie installs its audio daemon automatically the first time a session
starts (via the `SessionStart` bootstrap hook). Use this skill only when that
automatic install did not work or you want to reinstall.

## Steps

1. **Check what is already there**: Look for the compiled binary:
   - Linux/macOS: `${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon`
   - Windows: `${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon.exe`

   Read the bootstrap log at `~/.claude/.chappie_state/bootstrap.log` to see
   what the last automatic attempt did.

2. **Force a fresh download**: Delete the binary if present, then re-run the
   bootstrap script for the current OS:
   - Linux/macOS: `sh "${CLAUDE_PLUGIN_ROOT}/scripts/chappie-bootstrap.sh"`
   - Windows: `"${CLAUDE_PLUGIN_ROOT}/scripts/chappie-bootstrap.cmd"`

   This downloads the prebuilt binary for the user's platform from the GitHub
   Releases of `Mic-360/chappie`. No Rust toolchain is required.

3. **Build from source (fallback)**: Only if the download fails (for example,
   no network or no release published yet) and the user has Rust installed.
   Verify with `cargo --version`, then in `${CLAUDE_PLUGIN_ROOT}` run:
   ```
   cargo build --release
   ```
   If Rust is not installed, point the user to https://rustup.rs.

4. **Test the daemon**: Send a signal — this writes the signal file and
   launches the daemon if it is not already running:
   ```
   ${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon signal start
   ```
   You should hear mechanical keyboard typing within a second or two (sound
   assets are downloaded and cached on first run). Silence it with:
   ```
   ${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon signal quit
   ```

5. **Confirm**: Report whether the binary is in place and audio playback works.

The daemon logs to `~/.claude/.chappie_state/daemon.log`; the bootstrap logs to
`~/.claude/.chappie_state/bootstrap.log`.
```

- [ ] **Step 2: Verify the frontmatter parses**

Run: `python -c "import re; t=open('skills/setup/SKILL.md').read(); assert t.startswith('---'); assert 'name: setup' in t; print('ok')"`
Expected: `ok`

- [ ] **Step 3: Commit**

```bash
git add skills/setup/SKILL.md
git commit -m "docs: repurpose setup skill as a repair command"
```

---

### Task 7: Update the status skill

**Files:**
- Modify: `skills/status/SKILL.md`

- [ ] **Step 1: Replace the file content**

Replace the entire content of `skills/status/SKILL.md` with:

```markdown
---
name: status
description: Check the Chappie audio daemon status — whether the binary is installed, the daemon is running, sounds are cached, and how the binary was obtained.
disable-model-invocation: true
---

# Chappie Status

Check the status of the Chappie audio daemon:

1. **Binary**: Check whether the compiled daemon exists:
   - Linux/macOS: `${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon`
   - Windows: `${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon.exe`

2. **Bootstrap log**: Show the last 10 lines of
   `~/.claude/.chappie_state/bootstrap.log` if it exists — this reveals whether
   the binary was downloaded as a prebuilt release or built from source, and
   any failures.

3. **Daemon process**: Check if the daemon is running by reading
   `~/.claude/.chappie_state/daemon.pid` and verifying the process.

4. **Sound cache**: List files in `~/.claude/sounds/` to verify all 6 sound
   assets are downloaded (click1.wav through click4.wav, spacebar.wav,
   alert.wav).

5. **Signal state**: Read `~/.claude/.chappie_state/signal` to see the current
   signal.

6. **Daemon log**: Show the last 20 lines of
   `~/.claude/.chappie_state/daemon.log` if it exists.

Report findings concisely.
```

- [ ] **Step 2: Verify the frontmatter parses**

Run: `python -c "t=open('skills/status/SKILL.md').read(); assert t.startswith('---'); assert 'name: status' in t; print('ok')"`
Expected: `ok`

- [ ] **Step 3: Commit**

```bash
git add skills/status/SKILL.md
git commit -m "docs: report bootstrap state in status skill"
```

---

### Task 8: Add the maintainer release guide

**Files:**
- Create: `docs/RELEASING.md`

- [ ] **Step 1: Create the file**

Create `docs/RELEASING.md` with exactly this content:

```markdown
# Releasing Chappie

Chappie users never compile anything. The plugin's `SessionStart` hook
downloads a prebuilt `chappie-daemon` binary from this repository's GitHub
Releases. Cutting a release is therefore how you ship the daemon to users.

## One-time setup

Nothing — `.github/workflows/release.yml` already has the
`contents: write` permission it needs to publish Releases.

## Cutting a release

1. Bump the version in all three manifests so they agree:
   - `Cargo.toml` — `version`
   - `.claude-plugin/plugin.json` — `version`
   - `.claude-plugin/marketplace.json` — both `version` fields

2. Commit the bump:
   ```bash
   git add Cargo.toml .claude-plugin/plugin.json .claude-plugin/marketplace.json
   git commit -m "release: vX.Y.Z"
   ```

3. Tag and push:
   ```bash
   git tag vX.Y.Z
   git push origin main --tags
   ```

4. The `Release` workflow runs automatically on the `vX.Y.Z` tag. It builds
   five binaries and attaches them to a GitHub Release for that tag:

   | Asset | Platform |
   |---|---|
   | `chappie-daemon-linux-x86_64` | Linux, Intel/AMD 64-bit |
   | `chappie-daemon-linux-aarch64` | Linux, ARM64 |
   | `chappie-daemon-macos-x86_64` | macOS, Intel |
   | `chappie-daemon-macos-aarch64` | macOS, Apple Silicon |
   | `chappie-daemon-windows-x86_64.exe` | Windows (ARM64 via emulation) |

5. Verify on the repository's **Releases** page that all five assets are
   attached. The bootstrap scripts fetch
   `releases/latest/download/<asset>`, so the newest release is what users get.

## Testing the build without releasing

Trigger the workflow manually from the **Actions** tab
(`Release` -> `Run workflow`). On a manual run the `build` jobs run but the
`publish release` job is skipped (it only runs for `refs/tags/*`), so you can
validate that all five targets compile without creating a Release.

## If a build fails

Open the failed job in the **Actions** tab. Common causes:

- **Linux ALSA error** — the `libasound2-dev` install step failed; re-run the
  job.
- **macOS target missing** — `rustup target add` is handled by the toolchain
  action; re-run the job.

Re-run individual failed jobs from the workflow run page. Once green, the
`publish release` job attaches the assets.
```

- [ ] **Step 2: Commit**

```bash
git add docs/RELEASING.md
git commit -m "docs: add maintainer release guide"
```

---

### Task 9: Rewrite the README install flow

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the Installation section**

In `README.md`, replace the entire section from the `## Installation` heading
down to (but not including) the `## Plugin Structure` heading with:

````markdown
## Installation <img src="chappie.png" width="40" align="right">

Chappie needs **no build step and no Rust toolchain**. Install the plugin, and
the audio daemon downloads itself automatically the next time a Claude Code
session starts.

### From a Marketplace

If a marketplace includes Chappie:

```
/plugin install chappie@<marketplace-name>
```

### From this repository's marketplace

```
/plugin marketplace add Mic-360/chappie
/plugin install chappie@chappie-marketplace
```

### From GitHub directly

```
claude --plugin-url https://github.com/Mic-360/chappie
```

### That's it

After installing, **do nothing else**. On the next session, Chappie's
`SessionStart` hook runs a small bootstrap script that downloads the prebuilt
`chappie-daemon` binary for your platform (Windows, Linux, or macOS — Intel or
ARM) from the [GitHub Releases](https://github.com/Mic-360/chappie/releases)
and caches it inside the plugin. Start typing with Claude and you will hear it.

If Chappie stays silent, run `/chappie:setup` to repair the install, or check
`/chappie:status`. The bootstrap log lives at
`~/.claude/.chappie_state/bootstrap.log`.

### Build from source (contributors only)

You only need this if you are developing Chappie or are offline with no
published release available. It requires the [Rust toolchain](https://rustup.rs):

```bash
git clone https://github.com/Mic-360/chappie.git
cd chappie
cargo build --release
```

The bootstrap script also falls back to `cargo build` automatically if a
download fails and Rust is installed.

---
````

- [ ] **Step 2: Update the "How It Works" cross-platform note**

In `README.md`, find the paragraph under `### Architecture` that begins "The
hooks invoke the **same compiled binary**" and contains the phrase "no shell
scripts". Replace that paragraph with:

```markdown
The hooks invoke the **same compiled binary** in a lightweight `signal` mode
(`chappie-daemon signal <name>`). The binary itself is platform-specific, but
it is fetched automatically: a `SessionStart` hook runs one of two paired
bootstrap scripts — `scripts/chappie-bootstrap.sh` on Linux/macOS and
`scripts/chappie-bootstrap.cmd` on Windows. Whichever script does not match the
host fails harmlessly, so the correct one always runs. Each script resolves its
paths relative to the plugin directory, so the behavior is identical wherever
Claude Code installs the plugin.
```

- [ ] **Step 3: Update the Plugin Structure tree**

In `README.md`, replace the contents of the code block under
`## Plugin Structure` with:

```
chappie/
├── .claude-plugin/
│   ├── plugin.json            # Plugin manifest (name, version, author, etc.)
│   └── marketplace.json       # Marketplace definition for distribution
├── .github/
│   └── workflows/
│       └── release.yml        # CI: builds & publishes prebuilt binaries
├── hooks/
│   └── hooks.json             # Claude Code lifecycle hook definitions
├── scripts/
│   ├── chappie-bootstrap.sh   # Auto-download the daemon (Linux/macOS)
│   └── chappie-bootstrap.cmd  # Auto-download the daemon (Windows)
├── skills/
│   ├── setup/
│   │   └── SKILL.md           # /chappie:setup — repair / reinstall the daemon
│   └── status/
│       └── SKILL.md           # /chappie:status — check daemon health
├── src/
│   └── main.rs                # Rust audio daemon + `signal` hook entry point
├── docs/
│   └── RELEASING.md           # Maintainer release guide
├── Cargo.toml                 # Rust project manifest
├── LICENSE                    # MIT + third-party attribution
├── .gitignore
└── README.md
```

- [ ] **Step 4: Fix remaining `mic-360` references**

Run: `grep -rn "mic-360" README.md`
For each line returned, change `mic-360` to `Mic-360`. Then re-run the command.
Expected after fixing: no output.

- [ ] **Step 5: Verify the README still renders sensible structure**

Run: `grep -n "^## " README.md`
Expected: the section headings appear in order — `Features`, `Installation`,
`Plugin Structure`, `How It Works`, `Typing Rhythm`, `Tuning`, `Development`,
`Marketplace Distribution`, `License`.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "docs: rewrite install flow for one-command no-Rust setup"
```

---

## Self-Review Notes

- **Spec coverage:** Component 1 (CI) → Task 2; Component 2 (bootstrap scripts
  + hook wiring) → Tasks 3, 4, 5; Component 3 (skills + README + RELEASING +
  metadata casing) → Tasks 1, 6, 7, 8, 9. All spec sections are covered.
- **Asset names** are identical across `release.yml` (Task 2), the `.sh` script
  (Task 3), the `.cmd` script (Task 4), and `RELEASING.md` (Task 8):
  `chappie-daemon-{linux,macos}-{x86_64,aarch64}` and
  `chappie-daemon-windows-x86_64.exe`.
- **Paths** `target/release/chappie-daemon[.exe]` and the
  `~/.claude/.chappie_state/` log directory are consistent across all tasks and
  match the existing `src/main.rs` conventions.
- **No release exists yet:** until the first `vX.Y.Z` tag is pushed (per
  `RELEASING.md`), the bootstrap download 404s and falls back to source build —
  Task 3 step 4 explicitly accepts that outcome.

## Rollout note (post-implementation, maintainer action)

After all tasks merge, the maintainer must cut the first release by following
`docs/RELEASING.md` (push a `vX.Y.Z` tag). Until then, users without Rust will
see Chappie stay silent. This is a maintainer action outside the scope of these
code-change tasks.
```
