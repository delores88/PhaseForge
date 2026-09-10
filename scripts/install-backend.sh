#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }

if command -v apt-get >/dev/null; then
  sudo apt-get update
  sudo apt-get install -y build-essential pkg-config libvulkan1 vulkan-tools mesa-vulkan-drivers libsecret-1-0 dbus-user-session
elif command -v dnf >/dev/null; then
  sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config vulkan-loader vulkan-tools mesa-vulkan-drivers libsecret dbus-daemon
else
  echo "Install a C/C++ compiler, pkg-config, Vulkan loader/drivers, libsecret, and a user D-Bus secret service for your distribution." >&2
fi

if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
fi
rustup toolchain install stable --profile minimal
rustup default stable

cd "$ROOT/backend"
cargo fetch
cargo build
cargo test

echo "Backend installed. Start with ./scripts/start-backend.sh"
