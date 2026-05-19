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
