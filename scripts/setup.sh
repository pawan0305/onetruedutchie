#!/usr/bin/env bash
# Automated first-time setup for OneTrueDutchie on macOS.
# Run from the repo root: bash scripts/setup.sh
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
ok()   { echo -e "${GREEN}✓${NC} $*"; }
warn() { echo -e "${YELLOW}!${NC} $*"; }
die()  { echo -e "${RED}✗${NC} $*" >&2; exit 1; }
step() { echo -e "\n${YELLOW}▶ $*${NC}"; }

[[ "$(uname)" == "Darwin" ]] || die "this script is for macOS only."
[[ "$(uname -r)" < "22.0.0" ]] && die "requires macOS 13 (Ventura) or later."

# ── Xcode CLT ──────────────────────────────────────────────────────────────
step "Checking Xcode Command Line Tools..."
if ! xcode-select -p &>/dev/null; then
  warn "not found — installing now (this may take a few minutes)..."
  xcode-select --install
  echo "    A dialog appeared. Click Install, wait for it to finish, then run this script again."
  exit 0
fi
ok "Xcode CLT at $(xcode-select -p)"

# ── Swift / swiftc ──────────────────────────────────────────────────────────
step "Checking Swift..."
if ! command -v swiftc &>/dev/null; then
  die "swiftc not found. Install Xcode from the App Store or run 'xcode-select --install'."
fi
ok "$(swiftc --version 2>&1 | head -1)"

# ── Rust ────────────────────────────────────────────────────────────────────
step "Checking Rust..."
if ! command -v cargo &>/dev/null; then
  warn "Rust not found — installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
ok "$(cargo --version)"

# ── Node.js ─────────────────────────────────────────────────────────────────
step "Checking Node.js..."
if ! command -v node &>/dev/null; then
  die "Node.js not found. Install it:\n  brew install node  OR  nvm install 22\nThen re-run this script."
fi
NODE_MAJOR="$(node --version | grep -oE '[0-9]+' | head -1)"
(( NODE_MAJOR >= 20 )) || die "Node.js 20+ required (found $(node --version)). Upgrade with: nvm install 22"
ok "$(node --version)"

# ── npm install ─────────────────────────────────────────────────────────────
step "Installing npm dependencies..."
npm install
ok "npm dependencies installed"

# ── Swift audio sidecar ─────────────────────────────────────────────────────
step "Building Swift audio sidecar..."
npm run build:swift
ok "sidecar binary written to src-tauri/binaries/"

# ── Done ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  Setup complete!${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "  Next steps:"
echo ""
echo "  1. Run the app:  npm run tauri dev"
echo ""
echo "  2. When the Settings dialog opens, paste:"
echo "       Deepgram key  →  get one at https://console.deepgram.com/"
echo "       Anthropic key →  get one at https://console.anthropic.com/"
echo ""
echo "  3. Click 'Start meeting', approve Screen Recording + Mic permissions,"
echo "     then stop and restart the meeting (macOS needs a fresh process"
echo "     after granting permissions)."
echo ""
echo "  4. Open Teams, Zoom, or anything in another window."
echo "     Dutch speech appears as transcribed + translated text in seconds."
echo ""
