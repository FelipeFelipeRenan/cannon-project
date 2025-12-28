#!/bin/sh
# Cannon - High-Velocity Load Tester Installer

set -e

REPO="FelipeFelipeRenan/cannon-project"
BINARY_NAME="cannon"

# Detect OS
OS_TYPE=$(uname -s)
case "$OS_TYPE" in
    (Linux*)     ASSET_NAME="cannon-linux-x64";;
    (Darwin*)    ASSET_NAME="cannon-macos-x64";;
    (*)          echo "Error: OS $OS_TYPE not supported by this script."; exit 1;;
esac

echo "🚀 Finding latest release for $ASSET_NAME..."
LATEST_RELEASE_URL=$(curl -s https://api.github.com/repos/$REPO/releases/latest | grep "browser_download_url" | grep "$ASSET_NAME" | cut -d '"' -f 4)

if [ -z "$LATEST_RELEASE_URL" ]; then
    echo "Error: Could not find the latest release. Please check the repository."
    exit 1
fi

echo "📥 Downloading Cannon..."
curl -L "$LATEST_RELEASE_URL" -o $BINARY_NAME

echo "🔐 Setting permissions..."
chmod +x $BINARY_NAME

echo "📦 Moving binary to /usr/local/bin (requires sudo)..."
sudo mv $BINARY_NAME /usr/local/bin/$BINARY_NAME

echo "✅ Installation complete! Try running: cannon --help"