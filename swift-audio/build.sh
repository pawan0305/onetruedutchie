#!/usr/bin/env bash
# Build the AudioCapture Swift sidecar and copy it into src-tauri/binaries/
# with the platform-triple suffix Tauri's `externalBin` requires.
set -euo pipefail

cd "$(dirname "$0")"

ARCH="$(uname -m)"
case "$ARCH" in
  arm64)  TARGET_TRIPLE="aarch64-apple-darwin" ;;
  x86_64) TARGET_TRIPLE="x86_64-apple-darwin"  ;;
  *)      echo "unsupported arch: $ARCH" >&2; exit 1 ;;
esac

echo "==> swift build -c release  (target triple: $TARGET_TRIPLE)"
swift build -c release

OUT_DIR="../src-tauri/binaries"
mkdir -p "$OUT_DIR"
cp .build/release/AudioCapture "$OUT_DIR/audio-capture-$TARGET_TRIPLE"
chmod +x "$OUT_DIR/audio-capture-$TARGET_TRIPLE"

echo "==> wrote $OUT_DIR/audio-capture-$TARGET_TRIPLE"
