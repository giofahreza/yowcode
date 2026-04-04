# YowCode Feature Comparison

## Status: ✅ Compiles Successfully

Both CLI (`yowcode`) and Web (`yowcode-web`) binaries compile and are ready for testing.

## Feature Comparison

### ✅ Implemented Features

| Feature | Source | Implementation |
|---------|--------|----------------|
| **Core Architecture** | Both | Shared Rust library (core) with CLI and web binaries |
| **Session Management** | Both | SQLite-backed sessions shared between CLI and web |
| **Real-time Events** | Both | Broadcast channel for session updates |
| **Tool System** | Both | Extensible tool architecture with permission models |
| **AI Provider Support** | Both | Multi-provider (Anthropic, OpenAI, OpenRouter) |
| **Database Persistence** | Auto-dev | SQLite with migrations for sessions, messages, projects, runs |
| **CLI Interface** | Claude Code | ratatui-based TUI with chat input/output |
| **Web Interface** | Auto-dev | Axum-based web server with REST API |
| **WebSocket Support** | Auto-dev | Real-time chat updates via WebSocket |
| **Configuration Management** | Both | TOML config + environment variables |

### ⚠️ Partially Implemented

| Feature | Source | Status | Notes |
|---------|--------|--------|-------|
| **Tools** | Both | Basic set implemented | Bash, Read, Write, Edit, Glob, Grep |
| **Project Management** | Auto-dev | Types defined | UI and run execution not yet wired |
| **Runs/Tasks** | Auto-dev | Types defined | Execution loop not yet implemented |
| **Permission System** | Claude Code | Framework in place | Need to add interactive prompts |
| **Context Collection** | Both | Basic implementation | File walking and context building |

### ❌ Not Yet Implemented

#### From Auto-dev
- Web UI project registration and management
- GitHub webhook ingestion
- Run queue and background worker
- CrewAI orchestration
- Git worktree isolation
- Container execution modes
- Validation commands (test, lint, security, benchmark)
- Browser validation (Playwright, accessibility)
- PR creation and status updates
- Audit event tracking in UI

#### From Claude Code
- Slash commands (`/commit`, `/review`, `/mcp`, etc.)
- MCP server support
- Skills system
- Task management UI
- Keybindings and Vim mode
- Voice input
- Remote session support
- Memory directory (persistent memory)
- Agent spawning and coordination
- Plan mode
- IDE bridge integration
- Plugin system
- Theme switching

## Testing Notes

1. **CLI**: `./target/release/yowcode` - Starts the TUI interface
2. **Web**: `./target/release/yowcode-web` - Starts the web server on `http://127.0.0.1:3000`
3. **Configuration**: Set `YOWCODE_API_KEY` environment variable or create `~/.yowcode/config.toml`
4. **Database**: SQLite database at `~/.yowcode/yowcode.db`

## Next Steps for Full Feature Parity

To achieve full feature parity with auto-dev and claude-code, consider:

1. **Complete Tool Implementation**: Add all tools from Claude Code (~40 tools)
2. **Slash Commands**: Implement command parsing and execution
3. **MCP Integration**: Add Model Context Protocol server support
4. **Skills System**: Implement skill definition and execution
5. **Run Orchestration**: Complete the run/task execution loop
6. **Web UI Enhancements**: Add project management, run monitoring
7. **GitHub Integration**: Webhooks, PR creation, status updates
8. **Advanced Features**: Container isolation, validation hooks, browser testing
