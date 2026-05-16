---
name: status
description: Check the Chappie audio daemon status — whether it's running, if sounds are cached, and the current signal state.
disable-model-invocation: true
---

# Chappie Status

Check the status of the Chappie audio daemon:

1. **Daemon process**: Check if the daemon is running by reading `~/.claude/.chappie_state/daemon.pid` and verifying the process.

2. **Sound cache**: List files in `~/.claude/sounds/` to verify all 6 sound assets are downloaded (click1.wav through click4.wav, spacebar.wav, alert.wav).

3. **Signal state**: Read `~/.claude/.chappie_state/signal` to see the current signal.

4. **Daemon log**: Show the last 20 lines of `~/.claude/.chappie_state/daemon.log` if it exists.

Report findings concisely.
