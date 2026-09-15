#!/bin/sh
# Cannon - High-Velocity Load Tester Installer

set -eu

REPO="FelipeFelipeRenan/cannon-project"
BINARY_NAME="cannon"
INSTALL_DIR="/usr/local/bin"

# Detect OS
OS_TYPE=$(uname -s)

case "$OS_TYPE" in
    Linux*)
        ASSET_NAME="cannon-linux-x64"
        ;;
    Darwin*)
        ASSET_NAME="cannon-macos-x64"
        ;;
    *)
        echo "Error: OS $OS_TYPE is not supported by this script."
        exit 1
        ;;
esac

RELEASE_API="https://api.github.com/repos/$REPO/releases/latest"

echo "🚀 Finding latest release for $ASSET_NAME..."

RELEASE_JSON=$(curl --fail --silent --show-error --location "$RELEASE_API")

LATEST_RELEASE_URL=$(printf '%s\n' "$RELEASE_JSON" |
    grep '"browser_download_url"' |
    grep "\"$ASSET_NAME\"" |
    cut -d '"' -f 4)

CHECKSUM_URL=$(printf '%s\n' "$RELEASE_JSON" |
    grep '"browser_download_url"' |
    grep '"SHA256SUMS"' |
    cut -d '"' -f 4)

if [ -z "$LATEST_RELEASE_URL" ]; then
    echo "Error: Could not find the latest Cannon release binary."
    exit 1
fi

if [ -z "$CHECKSUM_URL" ]; then
    echo "Error: Could not find the SHA256SUMS release asset."
    exit 1
fi

TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT INT TERM

BINARY_PATH="$TEMP_DIR/$ASSET_NAME"
CHECKSUM_PATH="$TEMP_DIR/SHA256SUMS"

echo "📥 Downloading Cannon..."
curl --fail --silent --show-error --location \
    "$LATEST_RELEASE_URL" \
    -o "$BINARY_PATH"

echo "📥 Downloading checksums..."
curl --fail --silent --show-error --location \
    "$CHECKSUM_URL" \
    -o "$CHECKSUM_PATH"

echo "🔐 Verifying binary integrity..."

EXPECTED_CHECKSUM=$(grep "  $ASSET_NAME\$" "$CHECKSUM_PATH" | awk '{print $1}')

if [ -z "$EXPECTED_CHECKSUM" ]; then
    echo "Error: No checksum found for $ASSET_NAME."
    exit 1
fi

case "$OS_TYPE" in
    Linux*)
        ACTUAL_CHECKSUM=$(sha256sum "$BINARY_PATH" | awk '{print $1}')
        ;;
    Darwin*)
        ACTUAL_CHECKSUM=$(shasum -a 256 "$BINARY_PATH" | awk '{print $1}')
        ;;
esac

if [ "$ACTUAL_CHECKSUM" != "$EXPECTED_CHECKSUM" ]; then
    echo "❌ Checksum verification failed!"
    echo "Expected: $EXPECTED_CHECKSUM"
    echo "Actual:   $ACTUAL_CHECKSUM"
    exit 1
fi

echo "✅ Binary integrity verified."

chmod +x "$BINARY_PATH"

echo "📦 Installing Cannon to $INSTALL_DIR (requires sudo)..."
sudo mv "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"

echo "✅ Installation complete! Try running: cannon --help"