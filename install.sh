#!/bin/sh
set -e

REPO="2wee-dev/client"
BIN="2wee"
INSTALL_DIR="/usr/local/bin"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64)  PLATFORM="macos-arm64" ;;
      x86_64) PLATFORM="macos-x86_64" ;;
      *)      echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64)  PLATFORM="linux-x86_64" ;;
      aarch64) PLATFORM="linux-arm64" ;;
      *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

URL="https://github.com/${REPO}/releases/latest/download/${BIN}-${PLATFORM}"

echo "Downloading 2wee for ${PLATFORM}..."
curl -fsSL "$URL" -o "/tmp/${BIN}"
chmod +x "/tmp/${BIN}"

echo "Installing to ${INSTALL_DIR}/${BIN}..."
if [ -w "$INSTALL_DIR" ]; then
  mv "/tmp/${BIN}" "${INSTALL_DIR}/${BIN}"
else
  sudo mv "/tmp/${BIN}" "${INSTALL_DIR}/${BIN}"
fi

echo "Done. Run: 2wee https://your-app.com/terminal"
