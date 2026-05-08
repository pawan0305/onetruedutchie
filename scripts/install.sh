#!/usr/bin/env bash
# Build a release .app bundle, ad-hoc codesign it, and install to /Applications.
# After this, screen-recording / microphone permissions go to OneTrueDutchie itself
# instead of Terminal.
#
# Run from the repo root:  bash scripts/install.sh
set -euo pipefail

cd "$(dirname "$0")/.."

YELLOW='\033[1;33m'; GREEN='\033[0;32m'; RED='\033[0;31m'; NC='\033[0m'
step() { echo -e "\n${YELLOW}▶ $*${NC}"; }
ok()   { echo -e "${GREEN}✓${NC} $*"; }
die()  { echo -e "${RED}✗${NC} $*" >&2; exit 1; }

# Pick up rustup-installed cargo even in fresh shells.
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

# Make sure deps + sidecar exist; setup.sh is idempotent.
if [[ ! -d node_modules ]] || ! command -v cargo >/dev/null 2>&1; then
  step "First-time setup..."
  bash scripts/setup.sh
  [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
fi

# Make sure a stable code signing identity exists. Without one, ad-hoc signing
# changes the cdhash on every rebuild and macOS re-prompts for Screen Recording
# / Microphone permission every single time.
step "Checking code signing identity..."
bash scripts/setup-cert.sh

step "Building Swift audio sidecar..."
npm run build:swift

step "Building release app bundle (first run takes ~5-10 minutes)..."
npm run tauri build

step "Patching Info.plist with usage descriptions..."
bash scripts/inject-infoplist.sh

APP_SRC="src-tauri/target/release/bundle/macos/OneTrueDutchie.app"
[[ -d "$APP_SRC" ]] || die "build did not produce $APP_SRC"

step "Codesigning with stable identity (so TCC permissions persist)..."
codesign --force --deep --sign "OneTrueDutchie Local Dev" "$APP_SRC"
ok "signed"

APP_DEST="/Applications/OneTrueDutchie.app"
step "Installing to $APP_DEST..."
rm -rf "$APP_DEST"
cp -R "$APP_SRC" "$APP_DEST"
ok "installed"

# NOTE: We deliberately do NOT call `tccutil reset` here. Because the binary
# is signed with the same code signing identity every rebuild, macOS keeps
# the permission grant — the user only has to grant Screen Recording and
# Microphone once.

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  OneTrueDutchie installed at $APP_DEST${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "  Launch:    open '$APP_DEST'"
echo "  or open Launchpad / Spotlight and type 'OneTrueDutchie'."
echo ""
echo "  On first launch macOS will prompt for Screen Recording + Microphone."
echo "  Grant them, then quit (⌘Q) and relaunch — TCC only honours new"
echo "  permissions on a fresh process."
