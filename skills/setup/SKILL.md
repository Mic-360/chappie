---
name: setup
description: Build and configure the Chappie audio daemon. Run this after installing the plugin to compile the Rust binary and verify audio output works.
---

# Chappie Setup

You are setting up the Chappie mechanical keyboard sound plugin.

## Steps

1. **Check Rust toolchain**: Verify that `rustc` and `cargo` are installed by running `rustc --version` and `cargo --version`. If not installed, tell the user to install from https://rustup.rs.

2. **Build the daemon**: Navigate to `${CLAUDE_PLUGIN_ROOT}` and run:
   ```
   cargo build --release
   ```
   This compiles the `chappie-daemon` binary for the current platform.

3. **Verify the binary**: Check that the compiled binary exists:
   - Linux/macOS: `${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon`
   - Windows: `${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon.exe`

   The plugin's hooks invoke this binary directly (as `chappie-daemon signal <name>`),
   so there are no shell scripts to make executable — it works the same on
   Windows, macOS, and Linux.

4. **Test the daemon**: Send a signal — this writes the signal file *and*
   launches the daemon if it is not already running:
   ```
   ${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon signal start
   ```
   You should hear mechanical keyboard typing sounds within a second or two
   (sound assets are downloaded and cached on the first run). Silence it with:
   ```
   ${CLAUDE_PLUGIN_ROOT}/target/release/chappie-daemon signal quit
   ```

5. **Confirm**: Report whether the build succeeded and audio playback works.

If any step fails, diagnose the issue and help the user resolve it. The daemon
logs to `~/.claude/.chappie_state/daemon.log`.
