.PHONY: all build clean test run-cli run-web install help

# Default target
all: build

# Build all components
build:
	cargo build --release

# Build CLI only
build-cli:
	cargo build --release -p yowcode

# Build web server only
build-web:
	cargo build --release -p yowcode-web

# Clean build artifacts
clean:
	cargo clean

# Run tests
test:
	cargo test

# Run CLI
run-cli: build-cli
	./target/release/yowcode

# Run web server
run-web: build-web
	./target/release/yowcode-web

# Install binaries to ~/.local/bin
install: build
	install -m 755 target/release/yowcode ~/.local/bin/
	install -m 755 target/release/yowcode-web ~/.local/bin/

# Install CLI only
install-cli: build-cli
	install -m 755 target/release/yowcode ~/.local/bin/

# Install web server only
install-web: build-web
	install -m 755 target/release/yowcode-web ~/.local/bin/

# Format code
fmt:
	cargo fmt

# Check code
check:
	cargo check

# Run clippy
clippy:
	cargo clippy -- -D warnings

# Help
help:
	@echo "YowCode Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all        - Build all components (default)"
	@echo "  build-cli  - Build CLI only"
	@echo "  build-web  - Build web server only"
	@echo "  clean      - Clean build artifacts"
	@echo "  test       - Run tests"
	@echo "  run-cli    - Build and run CLI"
	@echo "  run-web    - Build and run web server"
	@echo "  install    - Install all binaries to ~/.local/bin"
	@echo "  fmt        - Format code"
	@echo "  check      - Check code without building"
	@echo "  clippy     - Run clippy linter"
	@echo "  help       - Show this help message"
