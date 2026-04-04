# YowCode

> A unified Rust codebase that combines CLI and Web UI for AI-powered development assistance, inspired by auto-dev and Claude Code.

## Overview

YowCode is a single-codebase application that provides both a terminal interface (CLI) and a web interface for interacting with AI coding assistants. Both interfaces share the same chat sessions, database, and tool execution system.

## Features

- **Dual Interface**: Run as a CLI with TUI or as a web server with browser UI
- **Shared Sessions**: Chat sessions persist and are accessible from both CLI and web
- **Tool System**: Extensible tool architecture for file operations, command execution, and more
- **Multi-Provider AI**: Support for Anthropic, OpenAI, and OpenRouter APIs
- **Project Management**: Register and manage multiple projects
- **Session Persistence**: SQLite-backed storage for sessions, messages, and runs
- **Real-time Updates**: WebSocket support for live chat updates in the web UI

## Architecture

```
yowcode/
├── core/           # Shared core library
│   ├── ai/         # AI client and streaming
│   ├── config/     # Configuration management
│   ├── context/    # Context collection and compression
│   ├── database/   # SQLite database and migrations
│   ├── error/      # Error types
│   ├── executor/   # Chat execution loop
│   ├── message/    # Message types and chat history
│   ├── session/    # Session management
│   ├── tool/       # Tool system and registry
│   └── types/      # Common types
├── cli/            # CLI binary with ratatui TUI
│   └── tools/      # CLI tool implementations
└── web/            # Web server binary with Axum
    ├── handlers/   # HTTP request handlers
    └── templates/  # HTML templates
```

## Installation

### Prerequisites

- Rust 1.82 or later
- SQLite3

### Build

```bash
# Build all components
cargo build --release

# Build CLI only
cargo build --release -p yowcode

# Build web server only
cargo build --release -p yowcode-web
```

## Configuration

Create a configuration file at `~/.yowcode/config.toml` or set environment variables:

```toml
[database]
path = "~/.yowcode/yowcode.db"

[ai]
provider = "anthropic"  # or "openai", "openrouter"
api_key = "your-api-key"
base_url = "https://api.anthropic.com/v1/messages"
model = "claude-sonnet-4-20250514"
max_tokens = 8192
temperature = 0.7

[server]
host = "127.0.0.1"
port = 3000
cors_origins = ["http://localhost:3000"]

[cli]
theme = "dark"
default_permission_mode = "default"
```

Or use environment variables:

```bash
export YOWCODE_API_KEY="your-api-key"
export YOWCODE_BASE_URL="https://api.anthropic.com/v1/messages"
export YOWCODE_MODEL="claude-sonnet-4-20250514"
export YOWCODE_DB_PATH="~/.yowcode/yowcode.db"
```

## Usage

### CLI Mode

```bash
# Run the CLI
cargo run --bin yowcode

# Or use the built binary
./target/release/yowcode
```

**CLI Controls:**
- `i` - Enter input mode
- `Esc` - Exit input mode
- `Enter` - Send message
- `q` - Quit
- `Ctrl+C` - Force quit

### Web Server Mode

```bash
# Start the web server
cargo run --bin yowcode-web

# Or use the built binary
./target/release/yowcode-web
```

Then open your browser to `http://localhost:3000`

### Shared Sessions

Sessions created in the CLI are accessible from the web UI and vice versa. Both interfaces read from and write to the same SQLite database.

## Tool System

YowCode includes built-in tools:

| Tool | Description | Destructive |
|------|-------------|-------------|
| `bash` | Execute shell commands | No |
| `read` | Read file contents | No |
| `write` | Write or create files | Yes |
| `edit` | Replace text in files | Yes |
| `glob` | Find files by pattern | No |
| `grep` | Search file contents | No |
| `commit` | Create git commits | Yes |
| `diff` | Show git diff | No |
| `ask_user` | Interactive user prompts | No |
| `sleep` | Delay execution | No |
| `synthetic_output` | Test output generation | No |
| `git_status` | Show git status | No |
| `git_branch` | List/create/checkout branches | Yes |
| `ls` | List directory contents | No |
| `file_info` | Get file metadata | No |

## Slash Commands (CLI)

The CLI supports slash commands for quick operations:

| Command | Description | Usage |
|---------|-------------|-------|
| `/help` | Show available commands | `/help [command]` |
| `/diff` | Show git diff | `/diff [staged]` |
| `/status` | Show git status | `/status` |
| `/commit` | Create git commit | `/commit <message>` |
| `/ls` | List directory | `/ls [path]` |
| `/cd` | Change directory | `/cd <path>` |
| `/pwd` | Show current directory | `/pwd` |
| `/clear` | Clear conversation | `/clear` |
| `/tools` | List available tools | `/tools` |

### Adding Custom Tools

```rust
use async_trait::async_trait;
use yowcode_core::tool::*;

pub struct MyTool;

#[async_trait]
impl ToolExecutor for MyTool {
    fn definition(&self) -> &Tool {
        &Self::definition()
    }

    async fn execute(
        &self,
        ctx: &ToolExecutionContext,
        params: serde_json::Value,
    ) -> Result<ToolResult> {
        // Your tool logic here
        Ok(ToolResult::success("my_tool", "Result"))
    }
}
```

## API Endpoints

### Sessions

- `GET /api/sessions` - List all sessions
- `POST /api/sessions` - Create a new session
- `GET /api/sessions/:id` - Get session details
- `DELETE /api/sessions/:id` - Delete a session
- `GET /api/sessions/:id/messages` - Get session messages
- `POST /api/sessions/:id/messages` - Send a message
- `PUT /api/sessions/:id/settings` - Update session settings

### Tools

- `GET /api/tools` - List available tools

### WebSocket

- `WS /ws` - Real-time chat and event updates

## Development

### Running Tests

```bash
cargo test
```

### Code Organization

- **Core Library (`core/`)**: Contains all shared business logic, types, and traits
- **CLI (`cli/`)**: Terminal UI implementation using ratatui
- **Web (`web/`)**: Web server implementation using Axum

### Database Schema

The SQLite database includes tables for:
- `sessions` - Chat sessions
- `messages` - Chat messages
- `projects` - Registered projects
- `runs` - Execution runs
- `tasks` - Tasks within runs
- `artifacts` - Generated artifacts
- `audit_events` - Audit trail

## License

MIT OR Apache-2.0

## Inspired By

- [auto-dev](https://github.com/your-org/auto-dev) - Multi-project agentic controller with web UI
- [Claude Code](https://claude.ai/code) - Anthropic's official CLI tool

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.
