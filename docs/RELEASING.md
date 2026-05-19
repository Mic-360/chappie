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
