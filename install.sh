#!/usr/bin/env bash
# NavForensics — installation / build (Linux / macOS)
#
#   Usage :
#     ./install.sh            # installe les dependances puis lance en dev
#     ./install.sh build      # installe puis construit le binaire / l'installeur
#     ./install.sh deps       # installe seulement les dependances
#
set -euo pipefail
ACTION="${1:-dev}"
cd "$(dirname "$0")"

cyan() { printf '\033[36m%s\033[0m\n' "$1"; }
yellow() { printf '\033[33m%s\033[0m\n' "$1"; }
green() { printf '\033[32m%s\033[0m\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }

cyan "== NavForensics - installation (Linux/macOS) =="

# --- Rust ---
if ! have cargo; then
    yellow "Rust absent : installation via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi
green "Rust : $(cargo --version)"

# --- Node ---
if ! have node; then
    yellow "Node.js absent."
    echo "Installez Node.js 18+ (nvm, ou le paquet de votre distribution) puis relancez." >&2
    exit 1
fi
green "Node : $(node --version)"

# --- Dependances systeme Tauri ---
if have apt-get; then
    cyan "Installation des dependances systeme Tauri (sudo apt-get)..."
    sudo apt-get update
    sudo apt-get install -y \
        libwebkit2gtk-4.1-dev build-essential curl wget file \
        libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
elif have dnf; then
    cyan "Installation des dependances systeme Tauri (sudo dnf)..."
    sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file \
        libappindicator-gtk3-devel librsvg2-devel gcc-c++
elif [ "$(uname)" = "Darwin" ]; then
    yellow "macOS : assurez-vous d'avoir les Xcode Command Line Tools (xcode-select --install)."
fi

# --- Dependances npm ---
cyan "Installation des dependances npm..."
npm install

# --- Icones ---
if [ ! -f src-tauri/icons/icon.png ] && [ -f assets/logo.png ]; then
    cyan "Generation des icones a partir de assets/logo.png..."
    npx tauri icon assets/logo.png
fi

case "$ACTION" in
    deps)
        green "Dependances installees. Lancez './install.sh dev' ou './install.sh build'."
        ;;
    build)
        cyan "Construction..."
        npm run tauri build
        green "Termine. Sortie : src-tauri/target/release/bundle/"
        ;;
    *)
        cyan "Lancement en mode developpement..."
        npm run tauri dev
        ;;
esac
