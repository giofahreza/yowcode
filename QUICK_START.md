# YowCode - Quick Start Guide

## What is YowCode?

YowCode is a Rust-based AI coding assistant with both CLI and Web UI interfaces, inspired by auto-dev and Claude Code.

**Key Features:**
- Single codebase for CLI and Web (shared Rust library)
- 16 built-in tools (bash, read, write, edit, git commands, etc.)
- 9 slash commands (help, diff, status, commit, cd, ls, pwd, clear, tools)
- Shared SQLite sessions between CLI and web
- Multi-provider AI support (Anthropic, OpenAI, OpenRouter)

## Quick Start

### 1. Install Dependencies

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### 2. Build

```bash
cd yowcode
cargo build --release
```

### 3. Configure

Set environment variables:

```bash
export YOWCODE_API_KEY="your-api-key"
export YOWCODE_BASE_URL="https://api.anthropic.com/v1/messages"
export YOWCODE_MODEL="claude-sonnet-4-20250514"
```

Or create `~/.yowcode/config.toml`:

```toml
[ai]
api_key = "your-api-key"
base_url = "https://api.anthropic.com/v1/messages"
model = "claude-sonnet-4-20250514"
```

### 4. Run CLI

```bash
./target/release/yowcode
```

**CLI Controls:**
- `i` - Enter input mode
- `Esc` - Exit input mode
- `Enter` - Send message
- `q` - Quit
- `Ctrl+C` - Force quit

### 5. Run Web Server

```bash
./target/release/yowcode-web
# Then open http://127.0.0.1:3000
```

## Available Tools

| Tool | Description | Usage |
|------|-------------|-------|
| `bash` | Execute shell commands | `bash command: "ls -la"` |
| `read` | Read file contents | `read path: "src/main.rs"` |
| `write` | Write/create file | `write path: "test.txt", content: "Hello"` |
| `edit` | Replace text in file | `edit path: "test.txt", old_string: "Hello", new_string: "Hi"` |
| `glob` | Find files by pattern | `glob pattern: "**/*.rs"` |
| `grep` | Search file contents | `grep pattern: "TODO"` |
| `commit` | Create git commit | `commit message: "Fix bug"` |
| `diff` | Show git diff | `diff` or `diff staged` |
| `git_status` | Show git status | `git_status` |
| `git_branch` | List/create branches | `git_branch` |
| `ls` | List directory | `ls path: "src/"` |
| `file_info` | Get file metadata | `file_info path: "Cargo.toml"` |
| `ask_user` | Interactive prompts | `ask_user question: "Continue?"` |
| `sleep` | Delay execution | `sleep seconds: 5` |

## Slash Commands (CLI Only)

| Command | Description | Usage |
|---------|-------------|-------|
| `/help` | Show commands | `/help` or `/help diff` |
| `/diff` | Show git diff | `/diff` or `/diff staged` |
| `/status` | Show git status | `/status` |
| `/commit` | Create commit | `/commit "Fix bug"` |
| `/ls` | List directory | `/ls` or `/ls src/` |
| `/cd` | Change directory | `/cd src/` |
| `/pwd` | Current directory | `/pwd` |
| `/clear` | Clear conversation | `/clear` |
| `/tools` | List tools | `/tools` |

## API Endpoints (Web)

- `GET /api/health` - Health check
- `GET /api/sessions` - List all sessions
- `POST /api/sessions` - Create new session
- `GET /api/sessions/:id` - Get session details
- `DELETE /api/sessions/:id` - Delete session
- `GET /api/sessions/:id/messages` - Get session messages
- `POST /api/sessions/:id/messages` - Send message
- `GET /api/tools` - List available tools
- `WS /ws` - WebSocket for real-time chat

## Example Session Flow

### CLI Example
```
i                                               # Enter input mode
/help                                           # Show available commands
/status                                         # Check git status
/commit "Initial commit"                        # Create commit
cd src                                         # Change to src directory
ls                                              # List files
i                                               # Enter AI chat mode
List all Rust files in this project           # Ask AI something
```

### Web Example
1. Open `http://127.0.0.1:3000`
2. Click "+ New Session"
3. Type your message and send
4. AI will respond and may use tools
5. Sessions persist - can be resumed from CLI

## Shared Sessions

Sessions are stored in `~/.yowcode/yowcode.db` (SQLite).

- Start a session in CLI
- Close CLI
- Open Web UI
- The same session is available
- Continue the conversation from either interface

## Project Structure

```
yowcode/
├── core/           # Shared library
│   ├── ai/         # AI clients
│   ├── commands/   # Slash commands
│   ├── tools/      # Tool implementations
│   ├── session/    # Session management
│   └── ...
├── cli/            # CLI binary (ratatui TUI)
│   └── tools.rs    # CLI tool implementations
└── web/            # Web server (Axum + WebSocket)
    ├── handlers.rs  # API handlers
    └── main.rs      # Server + WebSocket
```

## Next Steps

1. **Try the tools**: Test file operations, git commands
2. **Use commands**: Try `/help`, `/status`, `/diff`
3. **Web UI**: Open the browser interface
4. **Custom tools**: Add your own tools to the registry
5. **Custom commands**: Add new slash commands
6. **Project work**: Set up AI key and start coding

## Troubleshooting

**Build errors?**
- Ensure Rust 1.82+ is installed
- Run `cargo clean && cargo build --release`

**Database errors?**
- `mkdir -p ~/.yowcode && touch ~/.yowcode/yowcode.db`

**API errors?**
- Check `YOWCODE_API_KEY` is set
- Verify `YOWCODE_BASE_URL` is correct
- Ensure you have API credits
