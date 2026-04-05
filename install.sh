#!/bin/bash
# YowCode Installation Script
# Run this script to install YowCode on your system

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPO="giofahreza/yowcode"
VERSION="${1:-latest}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BINARY_NAME="yow"

echo -e "${BLUE}YowCode Installation Script${NC}"
echo "=================================="
echo ""

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"
echo -e "${YELLOW}Detecting platform...${NC} $OS $ARCH"

# Map architecture to binary name
case "$ARCH" in
    x86_64|amd64)
        ARCH_SUFFIX="amd64"
        ;;
    aarch64|arm64)
        ARCH_SUFFIX="arm64"
        ;;
    *)
        echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
        echo "Supported: x86_64, amd64, aarch64, arm64"
        exit 1
        ;;
esac

# Get latest version if not specified
if [ "$VERSION" = "latest" ]; then
    echo -e "${YELLOW}Fetching latest version...${NC}"
    VERSION=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep -o '"tag_name": *"[^"]*"' | grep -o '"[^"]*"$' | tr -d '"')
    if [ -z "$VERSION" ]; then
        echo -e "${RED}Error: Failed to fetch latest version${NC}"
        exit 1
    fi
    echo -e "${GREEN}Latest version: $VERSION${NC}"
fi

# Determine binary name for CLI
CLI_BINARY="yowcode-${VERSION}-linux-${ARCH_SUFFIX}.tar.gz"
if [ "$OS" = "Darwin" ]; then
    CLI_BINARY="yowcode-${VERSION}-darwin-${ARCH_SUFFIX}.tar.gz"
fi

# Download URL
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$CLI_BINARY"
echo -e "${YELLOW}Downloading from: $DOWNLOAD_URL${NC}"

# Download the binary
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"

echo ""
echo -e "${YELLOW}Downloading YowCode CLI...${NC}"
if ! curl -fsSL -o "$CLI_BINARY" "$DOWNLOAD_URL"; then
    echo -e "${RED}Error: Failed to download binary${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

# Download checksum
echo -e "${YELLOW}Downloading checksum...${NC}"
SHA256_FILE="$CLI_BINARY.sha256"
if ! curl -fsSL -o "$SHA256_FILE" "$DOWNLOAD_URL.sha256"; then
    echo -e "${YELLOW}Warning: Failed to download checksum, skipping verification${NC}"
else
    # Verify checksum
    echo -e "${YELLOW}Verifying checksum...${NC}"
    if ! sha256sum -c "$SHA256_FILE"; then
        echo -e "${RED}Error: Checksum verification failed!${NC}"
        rm -rf "$TEMP_DIR"
        exit 1
    fi
    echo -e "${GREEN}Checksum verified!${NC}"
fi

# Extract the binary
echo -e "${YELLOW}Extracting binary...${NC}"
tar -xzf "$CLI_BINARY"

# Find the binary (it might be named yowcode)
if [ -f "yowcode" ]; then
    mv yowcode yow
elif [ -f "yow" ]; then
    # Already named yow
    true
else
    echo -e "${RED}Error: Could not find binary in archive${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

# Make binary executable
chmod +x yow

# Install the binary
echo ""
echo -e "${YELLOW}Installing to $INSTALL_DIR/$BINARY_NAME...${NC}"

# Check if we have write permissions
if [ -w "$INSTALL_DIR" ]; then
    mv yow "$INSTALL_DIR/$BINARY_NAME"
else
    echo -e "${YELLOW}Need sudo to install to $INSTALL_DIR${NC}"
    sudo mv yow "$INSTALL_DIR/$BINARY_NAME"
fi

# Create config directory
echo -e "${YELLOW}Creating config directory...${NC}"
mkdir -p "$HOME/.yowcode"

# Create example config if it doesn't exist
if [ ! -f "$HOME/.yowcode/config.toml" ]; then
    echo -e "${YELLOW}Creating example config...${NC}"
    cat > "$HOME/.yowcode/config.toml" << 'EOF'
# YowCode Configuration

[database]
path = "~/.yowcode/yowcode.db"

[ai]
# Set your API key here or use YOWCODE_API_KEY environment variable
# api_key = "your-api-key-here"
# base_url = "https://api.anthropic.com/v1/messages"
# model = "claude-sonnet-4-20250514"
# max_tokens = 8192
# temperature = 0.7

[cli]
theme = "dark"
default_permission_mode = "default"

[server]
host = "127.0.0.1"
port = 3000
cors_origins = ["http://localhost:3000"]
EOF
fi

# Cleanup
cd -
rm -rf "$TEMP_DIR"

# Verify installation
if command -v "$BINARY_NAME" &> /dev/null; then
    echo ""
    echo -e "${GREEN}==================================${NC}"
    echo -e "${GREEN}YowCode installed successfully!${NC}"
    echo -e "${GREEN}==================================${NC}"
    echo ""
    echo "Version: $($BINARY_NAME --version)"
    echo "Location: $(which $BINARY_NAME)"
    echo ""
    echo -e "${BLUE}Quick Start:${NC}"
    echo "  1. Set your API key:"
    echo "     ${YELLOW}export YOWCODE_API_KEY=\"your-api-key\"${NC}"
    echo ""
    echo "  2. Run YowCode:"
    echo "     ${YELLOW}$BINARY_NAME${NC}"
    echo ""
    echo "  3. Or run in YOLO mode (auto-approve actions):"
    echo "     ${YELLOW}$BINARY_NAME --yolo${NC}"
    echo ""
    echo -e "${BLUE}Configuration:${NC}"
    echo "  Edit config: ${YELLOW}~/.yowcode/config.toml${NC}"
    echo "  Database:   ${YELLOW}~/.yowcode/yowcode.db${NC}"
    echo ""
    echo -e "${BLUE}Help:${NC}"
    echo "  ${YELLOW}$BINARY_NAME --help${NC}"
    echo ""
else
    echo -e "${RED}Installation completed but binary not in PATH${NC}"
    echo "Add $INSTALL_DIR to your PATH or run directly:"
    echo "  $INSTALL_DIR/$BINARY_NAME"
    exit 1
fi
