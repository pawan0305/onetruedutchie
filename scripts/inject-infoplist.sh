#!/usr/bin/env bash
# Merges the usage-description keys from src-tauri/Info.plist into the
# bundled .app's Info.plist after `npm run tauri build`. Re-run if you
# rebuild the app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src-tauri/Info.plist"
APP_DIR="$ROOT/src-tauri/target/release/bundle/macos/OneTrueDutchie.app"
DEST="$APP_DIR/Contents/Info.plist"

if [[ ! -d "$APP_DIR" ]]; then
  echo "App bundle not found at $APP_DIR — run 'npm run tauri build' first." >&2
  exit 1
fi

KEYS=(
  NSMicrophoneUsageDescription
  NSScreenCaptureUsageDescription
  NSAppleEventsUsageDescription
)

for KEY in "${KEYS[@]}"; do
  VAL="$(/usr/libexec/PlistBuddy -c "Print :$KEY" "$SRC" 2>/dev/null || true)"
  if [[ -z "$VAL" ]]; then continue; fi
  if /usr/libexec/PlistBuddy -c "Print :$KEY" "$DEST" >/dev/null 2>&1; then
    /usr/libexec/PlistBuddy -c "Set :$KEY \"$VAL\"" "$DEST"
  else
    /usr/libexec/PlistBuddy -c "Add :$KEY string \"$VAL\"" "$DEST"
  fi
  echo "  set $KEY"
done

echo "==> patched $DEST"
