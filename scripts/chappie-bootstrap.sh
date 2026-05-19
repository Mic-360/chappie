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
