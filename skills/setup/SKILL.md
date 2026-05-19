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
