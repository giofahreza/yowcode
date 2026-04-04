#!/bin/bash
# Basic usage example for YowCode

set -e

echo "YowCode - Basic Usage Example"
echo "=============================="
echo ""

# Check if built
if [ ! -f "target/release/yowcode" ] || [ ! -f "target/release/yowcode-web" ]; then
    echo "Building YowCode..."
    cargo build --release
fi

echo "1. CLI Mode"
echo "-----------"
echo "To run the CLI:"
echo "  ./target/release/yowcode"
echo ""
echo "Controls:"
echo "  i    - Enter input mode"
echo "  Esc  - Exit input mode"
echo "  Enter - Send message"
echo "  q    - Quit"
echo ""

echo "2. Web Server Mode"
echo "------------------"
echo "To run the web server:"
echo "  ./target/release/yowcode-web"
echo ""
echo "Then open: http://localhost:3000"
echo ""

echo "3. Configuration"
echo "----------------"
echo "Create ~/.yowcode/config.toml or use environment variables:"
echo "  export YOWCODE_API_KEY='your-api-key'"
echo "  export YOWCODE_MODEL='claude-sonnet-4-20250514'"
echo ""

echo "4. Shared Sessions"
echo "-----------------"
echo "Sessions created in CLI are accessible from web UI and vice versa."
echo "Both interfaces use the same SQLite database."
echo ""

echo "5. Available Tools"
echo "-----------------"
echo "  bash  - Execute shell commands"
echo "  read  - Read file contents"
echo "  write - Write or create files"
echo "  edit  - Replace text in files"
echo "  glob  - Find files by pattern"
echo "  grep  - Search file contents"
echo ""

echo "For more information, see README.md"
