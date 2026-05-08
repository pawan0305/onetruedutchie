#!/usr/bin/env bash
# One-command start. Runs setup.sh on first launch, then starts the dev app.
# Idempotent: skips work that's already done.
set -euo pipefail

cd "$(dirname "$0")/.."

YELLOW='\033[1;33m'; GREEN='\033[0;32m'; NC='\033[0m'

# Make sure rustup-installed cargo is on PATH for this shell.
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

needs_setup=0
[[ ! -d node_modules ]] && needs_setup=1
[[ ! -f src-tauri/binaries/audio-capture-aarch64-apple-darwin ]] && \
  [[ ! -f src-tauri/binaries/audio-capture-x86_64-apple-darwin ]] && needs_setup=1
command -v cargo >/dev/null 2>&1 || needs_setup=1

if (( needs_setup )); then
  echo -e "${YELLOW}▶ First-time setup...${NC}"
  bash scripts/setup.sh
  # Re-source cargo env in case rustup just installed it.
  [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
fi

echo -e "${GREEN}▶ Starting OneTrueDutchie...${NC}"
exec npm run tauri dev
