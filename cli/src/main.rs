use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
    layout::Position,
};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::info;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;
use yowcode_core::{
    ai::{AIConfig, AIProvider, OpenAICompatClient},
    commands::{self, CommandContext, CommandRegistry, parse_command, execute_command, find_closest_command, register_default_commands},
    config::Config,
    database::initialize_database,
    executor::{ChatExecutor, ExecutionEvent, ExecutionOptions},
    message::{ChatHistory, Message, MessageContent, MessageRole},
    session::{Session, SessionManager, SessionSettings},
    tool::ToolRegistry,
    types::{InterfaceMode, PermissionMode},
};

mod tools;

/// YowCode - AI-powered coding assistant
#[derive(Parser, Debug)]
#[command(name = "yow", version, author, about)]
struct Args {
    /// Enable YOLO mode - auto-approve all actions (use with caution!)
    #[arg(long)]
    yolo: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Set AI model (opus, sonnet, haiku)
    #[arg(long)]
    model: Option<String>,

    /// Set effort level (low, normal, high)
    #[arg(long)]
    effort: Option<String>,

    /// List all commands
    #[arg(long)]
    list_commands: bool,

    /// Show current configuration
    #[arg(long)]
    show_config: bool,
}

/// Main application state
struct App {
    session_id: Uuid,
    messages: Vec<(MessageRole, String)>,
    input: String,
    cursor_position: usize,
    current_directory: PathBuf,
    session_manager: Arc<SessionManager>,
    executor: Arc<ChatExecutor>,
    tool_registry: Arc<ToolRegistry>,
    command_registry: Arc<CommandRegistry>,
    status: String,
    is_thinking: bool,
    config: Config,
    show_command_hints: bool,
    current_mode: InterfaceMode,
    current_effort: String,
    current_model: String,
    selected_hint_index: usize,  // For navigating command hints
    _hints_were_shown: bool,     // Track if hints were shown to avoid resetting index
    scroll_offset: usize,        // For scrolling messages
    input_history: Vec<String>,  // Command history for arrow up/down
    history_index: usize,        // Current position in history
}

impl App {
    fn new(
        session_id: Uuid,
        session_manager: Arc<SessionManager>,
        executor: Arc<ChatExecutor>,
        tool_registry: Arc<ToolRegistry>,
        command_registry: Arc<CommandRegistry>,
        config: Config,
    ) -> Self {
        let current_mode = InterfaceMode::CLI;
        let current_model = config.ai.model.clone();
        Self {
            session_id,
            messages: Vec::new(),
            input: String::new(),
            cursor_position: 0,
            current_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            session_manager,
            executor,
            tool_registry,
            command_registry,
            status: "Ready".to_string(),
            is_thinking: false,
            config,
            show_command_hints: false,
            current_mode,
            current_effort: "normal".to_string(),
            current_model,
            selected_hint_index: 0,
            _hints_were_shown: false,
            scroll_offset: 0,
            input_history: Vec::new(),
            history_index: 0,
        }
    }

    async fn load_history(&mut self) {
        if let Ok(history) = self.session_manager.get_history(self.session_id).await {
            self.messages = history.messages.iter().map(|msg| {
                let role = match msg.role {
                    yowcode_core::message::MessageRole::User => MessageRole::User,
                    yowcode_core::message::MessageRole::Assistant => MessageRole::Assistant,
                    yowcode_core::message::MessageRole::System => MessageRole::System,
                    yowcode_core::message::MessageRole::Tool => MessageRole::Tool,
                };
                let content = match &msg.content {
                    yowcode_core::message::MessageContent::Text(text) => text.clone(),
                    yowcode_core::message::MessageContent::TextWithImages { text, .. } => text.clone(),
                    yowcode_core::message::MessageContent::ToolCalls(calls) => {
                        calls.iter().map(|c| format!("Tool call: {} with args: {}", c.name, c.arguments)).collect::<Vec<_>>().join("\n")
                    }
                    yowcode_core::message::MessageContent::ToolResult { content, is_error, .. } => {
                        if *is_error {
                            format!("Tool error: {}", content)
                        } else {
                            format!("Tool result: {}", content)
                        }
                    }
                    yowcode_core::message::MessageContent::Thinking(text) => format!("Thinking: {}", text),
                };
                (role, content)
            }).collect();
        }
    }

    fn add_message(&mut self, role: MessageRole, content: String) {
        self.messages.push((role, content));
    }

    fn get_input_width(&self) -> usize {
        self.input.width()
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_position < self.get_input_width() {
            self.cursor_position += 1;
        }
    }

    fn move_cursor_to_start(&mut self) {
        self.cursor_position = 0;
    }

    fn move_cursor_to_end(&mut self) {
        self.cursor_position = self.get_input_width();
    }

    fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    fn delete_char(&mut self) {
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
        }
    }

    fn backspace(&mut self) {
        if self.cursor_position > 0 {
            let before = self.cursor_position.saturating_sub(1);
            self.input.remove(before);
            self.cursor_position = before;
        }
    }

    fn check_command_hints(&mut self) {
        // Show hints if input starts with /
        let should_show = self.input.starts_with('/') && self.input.len() <= 20;

        // Only reset index when hints transition from false to true
        if should_show && !self._hints_were_shown {
            self.selected_hint_index = 0;
        } else if should_show {
            // Adjust index if it's out of bounds after filtering
            let matches = self.get_matching_commands();
            if !matches.is_empty() && self.selected_hint_index >= matches.len() {
                self.selected_hint_index = matches.len() - 1;
            }
        }

        self.show_command_hints = should_show;
        self._hints_were_shown = should_show;
    }

    fn navigate_history_up(&mut self) {
        if !self.input_history.is_empty() {
            // Save current input if we're not already browsing history
            if self.history_index == 0 && !self.input.is_empty() {
                self.input_history.insert(0, self.input.clone());
                self.history_index = 1;
            } else if self.history_index < self.input_history.len() {
                self.history_index += 1;
            }
            self.input = self.input_history.get(self.history_index - 1).cloned().unwrap_or_default();
            self.cursor_position = self.input.len();
        }
    }

    fn navigate_history_down(&mut self) {
        if self.history_index > 0 {
            self.history_index -= 1;
            if self.history_index == 0 {
                self.input = String::new();
            } else {
                self.input = self.input_history.get(self.history_index - 1).cloned().unwrap_or_default();
            }
            self.cursor_position = self.input.len();
        }
    }

    fn add_to_history(&mut self) {
        if !self.input.is_empty() {
            self.input_history.insert(0, self.input.clone());
            self.history_index = 0;
        }
    }

    fn scroll_up(&mut self) {
        // Scroll up means see older messages (increase offset)
        self.scroll_offset += 1;
    }

    fn scroll_down(&mut self) {
        // Scroll down means see newer messages (decrease offset, but not below 0)
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn get_matching_commands(&self) -> Vec<(String, String)> {
        if !self.input.starts_with('/') {
            return Vec::new();
        }

        let search = self.input[1..].to_lowercase();
        let all_commands = vec![
            ("/help", "Show available commands"),
            ("/diff", "Show git diff"),
            ("/status", "Show git repository status"),
            ("/commit", "Create a git commit"),
            ("/ls", "List directory contents"),
            ("/cd", "Change directory"),
            ("/pwd", "Print working directory"),
            ("/clear", "Clear the screen"),
            ("/undo", "Undo last action"),
            ("/tools", "List available tools"),
            ("/cost", "Show token usage and costs"),
            ("/config", "Show or set configuration"),
            ("/mode", "Show or set permission mode"),
            ("/effort", "Show or set AI effort level"),
            ("/model", "List or change AI model"),
            ("/mcp", "Manage MCP servers"),
            ("/agent", "Manage AI agents"),
            ("/skill", "Manage skills"),
            ("/memory", "Manage memory"),
        ];

        all_commands
            .into_iter()
            .filter(|(cmd, _)| cmd.starts_with(&format!("/{}", search)))
            .map(|(cmd, desc)| (cmd.to_string(), desc.to_string()))
            .collect()
    }

    fn find_closest_command(&self, input: &str) -> Option<String> {
        find_closest_command(input)
    }

    async fn send_message(&mut self) -> Result<()> {
        if self.input.is_empty() {
            return Ok(());
        }

        let query = self.input.clone();
        self.add_to_history();
        self.add_message(MessageRole::User, query.clone());
        self.input.clear();
        self.cursor_position = 0;
        self.show_command_hints = false;
        self.scroll_offset = 0; // Reset scroll to bottom when new message added

        // Check if this is a command
        if let Some((cmd_name, _cmd_args)) = parse_command(&query) {
            // Special handling for undo command
            if cmd_name == "undo" {
                if !self.messages.is_empty() {
                    self.messages.pop();
                    self.status = "Last message removed".to_string();
                } else {
                    self.status = "Nothing to undo".to_string();
                }
                return Ok(());
            }

            self.status = format!("Running /{}...", cmd_name);

            let mut cmd_ctx = CommandContext::new(
                self.session_id,
                self.session_manager.clone(),
                self.tool_registry.clone(),
                self.command_registry.clone(),
            );
            cmd_ctx.current_directory = self.current_directory.clone();

            match execute_command(&query, &mut cmd_ctx, &self.command_registry).await {
                Ok(cmd_result) => {
                    self.current_directory = cmd_ctx.current_directory.clone();

                    // Apply state changes from command
                    if let Some(model) = cmd_result.changed_model {
                        self.current_model = model;
                    }
                    if let Some(_mode) = cmd_result.changed_mode {
                        // Permission mode change - would need to update session settings
                        // For now, just show in message
                    }
                    if let Some(effort) = cmd_result.changed_effort {
                        self.current_effort = effort;
                    }

                    self.status = if cmd_result.is_error {
                        "Command failed".to_string()
                    } else {
                        "Command completed".to_string()
                    };

                    if cmd_result.is_error {
                        self.add_message(MessageRole::System, format!("Error: {}", cmd_result.output));
                    } else {
                        self.add_message(MessageRole::System, cmd_result.output);
                    }
                }
                Err(e) => {
                    // Check if this is an unknown command and try to find the closest match
                    if query.starts_with('/') {
                        if let Some(closest_cmd) = self.find_closest_command(&query) {
                            // Auto-execute the closest command
                            let mut cmd_ctx = CommandContext::new(
                                self.session_id,
                                self.session_manager.clone(),
                                self.tool_registry.clone(),
                                self.command_registry.clone(),
                            );
                            cmd_ctx.current_directory = self.current_directory.clone();

                            match execute_command(&closest_cmd, &mut cmd_ctx, &self.command_registry).await {
                                Ok(cmd_result) => {
                                    self.current_directory = cmd_ctx.current_directory.clone();

                                    // Apply state changes from command
                                    if let Some(model) = cmd_result.changed_model {
                                        self.current_model = model;
                                    }
                                    if let Some(_mode) = cmd_result.changed_mode {
                                        // Permission mode change - would need to update session settings
                                    }
                                    if let Some(effort) = cmd_result.changed_effort {
                                        self.current_effort = effort;
                                    }

                                    self.status = if cmd_result.is_error {
                                        "Command failed".to_string()
                                    } else {
                                        "Command completed".to_string()
                                    };

                                    if cmd_result.is_error {
                                        self.add_message(MessageRole::System, format!("Error: {}", cmd_result.output));
                                    } else {
                                        self.add_message(MessageRole::System, cmd_result.output);
                                    }
                                }
                                Err(e2) => {
                                    self.status = "Command error".to_string();
                                    self.add_message(MessageRole::System, format!("Command error (tried {}): {}", closest_cmd, e2));
                                }
                            }
                        } else {
                            self.status = "Command error".to_string();
                            self.add_message(MessageRole::System, format!("Command error: {}", e));
                        }
                    } else {
                        self.status = "Command error".to_string();
                        self.add_message(MessageRole::System, format!("Command error: {}", e));
                    }
                }
            }
            return Ok(());
        }

        // Not a command, proceed with AI chat
        self.is_thinking = true;
        self.status = "Thinking...".to_string();

        // Get or create history
        let mut history = self
            .session_manager
            .get_history(self.session_id)
            .await
            .unwrap_or_else(|_| ChatHistory::new(self.session_id));

        // Collect events during execution
        let mut events = Vec::new();

        // Execute the query
        let result = self
            .executor
            .execute(
                &mut history,
                query,
                ExecutionOptions::default(),
                |event| {
                    events.push(event);
                },
            )
            .await;

        self.is_thinking = false;

        // Process collected events
        for event in events {
            match event {
                ExecutionEvent::Iteration { iteration } => {
                    self.status = format!("Iteration {}", iteration);
                }
                ExecutionEvent::AIThinking => {
                    self.status = "AI thinking...".to_string();
                }
                ExecutionEvent::AssistantMessage { content } => {
                    self.add_message(MessageRole::Assistant, content);
                }
                ExecutionEvent::ToolCall { tool_name } => {
                    self.add_message(MessageRole::System, format!("Running: {}", tool_name));
                }
                ExecutionEvent::ToolResult {
                    tool_name,
                    result,
                    is_error,
                } => {
                    let prefix = if is_error { "Error" } else { "Result" };
                    self.add_message(
                        MessageRole::System,
                        format!("{} from {}: {}", prefix, tool_name, result),
                    );
                }
                ExecutionEvent::TokenUsage {
                    prompt,
                    completion,
                    total,
                } => {
                    self.status = format!("Tokens: {} + {} = {}", prompt, completion, total);
                }
                _ => {}
            }
        }

        match result {
            Ok(exec_result) => {
                self.status = format!("Done ({} iterations, {}ms)", exec_result.iteration_count, exec_result.duration_ms);

                // Save messages to session
                for (role, content) in &self.messages {
                    let _ = self.session_manager.add_message(
                        self.session_id,
                        Message::new(*role, MessageContent::Text(content.clone())),
                    ).await;
                }
            }
            Err(e) => {
                self.status = format!("Error: {}", e);
                self.add_message(MessageRole::System, format!("Error: {}", e));
            }
        }

        Ok(())
    }
}

/// Run the TUI application
async fn run_tui(mut app: App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup event loop
    let tick_rate = Duration::from_millis(100);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        // Check for command hints after input changes
        app.check_command_hints();

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                // Check for Ctrl+C and Ctrl+Q first, before general Char handling
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('c') | KeyCode::Char('q') => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Enter => {
                            if app.show_command_hints {
                                let matches = app.get_matching_commands();
                                if !matches.is_empty() && app.selected_hint_index < matches.len() {
                                    // Execute the selected command
                                    app.input = matches[app.selected_hint_index].0.clone();
                                    app.cursor_position = app.input.len();
                                    app.show_command_hints = false;
                                    app.selected_hint_index = 0;
                                }
                                app.send_message().await?;
                                // Immediately redraw to show the new message
                                terminal.draw(|f| ui(f, &app))?;
                            } else {
                                app.send_message().await?;
                                // Immediately redraw to show the new message and thinking state
                                terminal.draw(|f| ui(f, &app))?;
                            }
                        }
                        KeyCode::Char(c) => {
                            app.insert_char(c);
                        }
                        KeyCode::Backspace => {
                            app.backspace();
                        }
                        KeyCode::Delete => {
                            app.delete_char();
                        }
                        KeyCode::Left => {
                            app.move_cursor_left();
                        }
                        KeyCode::Right => {
                            app.move_cursor_right();
                        }
                        KeyCode::Up => {
                            // Navigate command hints up or history
                            if app.show_command_hints {
                                let matches = app.get_matching_commands();
                                if !matches.is_empty() && app.selected_hint_index > 0 {
                                    app.selected_hint_index -= 1;
                                }
                            } else {
                                app.navigate_history_up();
                            }
                        }
                        KeyCode::Down => {
                            // Navigate command hints down or history
                            if app.show_command_hints {
                                let matches = app.get_matching_commands();
                                if !matches.is_empty() && app.selected_hint_index < matches.len().saturating_sub(1) {
                                    app.selected_hint_index += 1;
                                }
                            } else {
                                app.navigate_history_down();
                            }
                        }
                        KeyCode::Home => {
                            app.move_cursor_to_start();
                        }
                        KeyCode::End => {
                            app.move_cursor_to_end();
                        }
                        KeyCode::PageUp => {
                            app.scroll_up();
                        }
                        KeyCode::PageDown => {
                            app.scroll_down();
                        }
                        KeyCode::Tab => {
                            // Auto-complete command
                            if app.input.starts_with('/') {
                                let matches = app.get_matching_commands();
                                if matches.len() == 1 {
                                    app.input = matches[0].0.clone();
                                    app.cursor_position = app.input.len();
                                    app.show_command_hints = false;
                                    app.selected_hint_index = 0;
                                }
                            }
                        }
                        KeyCode::Esc => {
                            app.show_command_hints = false;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// UI rendering - Claude Code style
fn ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Split into: messages area, status bar, input area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Status bar (top)
            Constraint::Min(0),      // Messages (flexible)
            Constraint::Length(1),  // Input area (bottom)
        ])
        .split(size);

    // Render status bar
    let status_text = format!(
        "{} | {} | {} | {}",
        app.current_mode,
        app.current_model,
        app.current_effort,
        app.status
    );

    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

    f.render_widget(status_bar, chunks[0]);

    // Render messages with scroll support
    let messages_text: String = app
        .messages
        .iter()
        .map(|(role, content)| {
            let prefix = match role {
                MessageRole::User => "You:",
                MessageRole::Assistant => "AI:",
                MessageRole::System => "System:",
                MessageRole::Tool => "Tool:",
            };
            format!("{}\n{}", prefix, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Calculate how many lines to show and apply scroll offset
    let message_lines: Vec<&str> = messages_text.lines().collect();
    let available_height = chunks[1].height as usize;

    let start_line = if message_lines.len() > available_height {
        message_lines.len().saturating_sub(available_height + app.scroll_offset)
    } else {
        0
    };

    let visible_lines: Vec<&str> = message_lines
        .iter()
        .skip(start_line)
        .take(available_height)
        .copied()
        .collect();

    let visible_text = visible_lines.join("\n");

    let messages_widget = Paragraph::new(visible_text)
        .style(Style::default().fg(Color::Reset))
        .wrap(Wrap { trim: false });

    f.render_widget(messages_widget, chunks[1]);

    // Render input with multi-line support
    let input_lines: Vec<String> = textwrap::wrap(&app.input, chunks[2].width.saturating_sub(2) as usize)
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    let input_text = if input_lines.is_empty() {
        "⟩ ".to_string()
    } else {
        format!("⟩ {}", input_lines.join("\n  "))
    };

    let input_widget = Paragraph::new(input_text)
        .style(Style::default().fg(Color::Reset));

    f.render_widget(input_widget, chunks[2]);

    // Calculate cursor position for multi-line input
    let input_before_cursor = &app.input[..app.cursor_position.min(app.input.len())];
    let wrapped_before = textwrap::wrap(input_before_cursor, chunks[2].width.saturating_sub(2) as usize);
    let line_count = wrapped_before.len();

    let last_line = wrapped_before.last().map(|s| s.as_ref()).unwrap_or("");
    let cursor_x = (last_line.len() + 2) as u16; // +2 for "⟩ "
    let cursor_y = (line_count.saturating_sub(1)) as u16;

    // Set cursor position
    if cursor_x < chunks[2].width && cursor_y < chunks[2].height {
        f.set_cursor_position(Position::new(
            chunks[2].x + cursor_x,
            chunks[2].y + cursor_y,
        ));
    }

    // Show scroll indicator if needed
    if app.scroll_offset > 0 {
        // We've scrolled up, so we can scroll down to see newer messages
        let scroll_indicator = Paragraph::new("↓ (scroll down for newer)")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Left);

        let scroll_area = ratatui::layout::Rect {
            x: chunks[1].x,
            y: chunks[1].y + chunks[1].height - 1,
            width: 25,
            height: 1,
        };

        f.render_widget(scroll_indicator, scroll_area);
    }

    if message_lines.len() > available_height + app.scroll_offset {
        // There are more messages above that we can scroll up to see
        let scroll_indicator = Paragraph::new("↑ (scroll up for older)")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Left);

        let scroll_area = ratatui::layout::Rect {
            x: chunks[1].x,
            y: chunks[1].y,
            width: 25,
            height: 1,
        };

        f.render_widget(scroll_indicator, scroll_area);
    }

    // Show command hints popup
    if app.show_command_hints && !app.input.is_empty() {
        let matches = app.get_matching_commands();
        if !matches.is_empty() {
            let hint_items: Vec<ListItem> = matches
                .iter()
                .enumerate()
                .map(|(i, (cmd, desc))| {
                    let is_selected = i == app.selected_hint_index;
                    let style = if is_selected {
                        Style::default().bg(Color::Blue).fg(Color::White)
                    } else {
                        Style::default().fg(Color::Reset)
                    };
                    ListItem::new(format!("{} - {}", cmd, desc)).style(style)
                })
                .collect();

            // Calculate max width for the popup
            let max_width = matches.iter()
                .map(|(cmd, desc)| cmd.width() + 2 + desc.width())
                .max()
                .unwrap_or(30) as u16 + 4;

            let hint_list = List::new(hint_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray))
                )
                .highlight_style(Style::default().add_modifier(Modifier::BOLD));

            let popup_area = ratatui::layout::Rect {
                x: chunks[2].x,
                y: chunks[2].y.saturating_sub(matches.len() as u16 + 2),
                width: max_width.min(chunks[2].width),
                height: matches.len() as u16 + 2,
            };

            // Ensure popup stays within screen bounds
            let popup_area = ratatui::layout::Rect {
                y: popup_area.y.min(size.height.saturating_sub(popup_area.height)),
                ..popup_area
            };

            f.render_widget(Clear, popup_area);
            f.render_widget(hint_list, popup_area);
        }
    }

    // Subtle thinking indicator in top-right corner
    if app.is_thinking {
        let thinking = Paragraph::new("…")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Right);

        let thinking_area = ratatui::layout::Rect {
            x: size.width.saturating_sub(2),
            y: size.y,
            width: 2,
            height: 1,
        };

        f.render_widget(thinking, thinking_area);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize logging
    let log_filter = if args.verbose {
        "yowcode=debug,info"
    } else {
        "yowcode=info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .init();

    if args.yolo {
        println!("🚀 YOLO MODE ENABLED - Auto-approving all actions!");
    }

    info!("Starting YowCode CLI (YOLO: {})", args.yolo);

    // Handle CLI-level commands that don't need config
    if args.list_commands {
        println!("Available commands:");
        println!("  /help          - Show available commands");
        println!("  /diff          - Show git diff");
        println!("  /status        - Show git repository status");
        println!("  /commit        - Create a git commit");
        println!("  /ls            - List directory contents");
        println!("  /cd            - Change directory");
        println!("  /pwd           - Print working directory");
        println!("  /clear         - Clear the screen");
        println!("  /undo          - Undo last action");
        println!("  /tools         - List available tools");
        println!("  /cost          - Show token usage and costs");
        println!("  /config        - Show or set configuration");
        println!("  /mode          - Show or set permission mode");
        println!("  /effort        - Show or set AI effort level");
        println!("  /model         - List or change AI model");
        println!("  /mcp           - Manage MCP servers");
        println!("  /agent         - Manage AI agents");
        println!("  /skill         - Manage skills");
        println!("  /memory        - Manage memory");
        return Ok(());
    }

    if let Some(effort) = args.effort {
        match effort.as_str() {
            "low" => println!("Effort set to: low (faster responses)"),
            "normal" => println!("Effort set to: normal (balanced)"),
            "high" => println!("Effort set to: high (more thorough)"),
            _ => println!("Unknown effort level: {}. Use: low, normal, or high", effort),
        }
        return Ok(());
    }

    // Load configuration
    let config = Config::load(None).await.context("Failed to load configuration")?;

    // Handle CLI-level commands that need config
    if args.show_config {
        println!("Current configuration:");
        println!("  Model: {}", config.ai.model);
        println!("  Max tokens: {}", config.ai.max_tokens);
        println!("  Temperature: {}", config.ai.temperature);
        println!("  Database: {}", config.database.path);
        println!("  Theme: {}", config.cli.theme);
        return Ok(());
    }

    if let Some(model) = args.model {
        println!("Setting AI model to: {}", model);
        // TODO: Actually persist this to config
        return Ok(());
    }

    // Expand database path
    let db_path = Config::expand_path(&config.database.path);
    let db_path_str = db_path.to_string_lossy().to_string();

    // Initialize database
    let db = initialize_database(&db_path_str)
        .await
        .context("Failed to initialize database")?;

    // Create session manager
    let session_manager = Arc::new(SessionManager::new(db.clone()));

    // Create AI client
    let ai_config = AIConfig {
        provider: match config.ai.provider.as_str() {
            "openai" => AIProvider::OpenAI,
            "anthropic" => AIProvider::Anthropic,
            "openrouter" => AIProvider::OpenRouter,
            _ => AIProvider::Custom,
        },
        api_key: config.ai.api_key.clone(),
        base_url: config.ai.base_url.clone(),
        model: config.ai.model.clone(),
        max_retries: 3,
        timeout: std::time::Duration::from_secs(120),
    };

    let ai_client = Arc::new(OpenAICompatClient::new(ai_config));

    // Create tool registry and register tools
    let mut tool_registry = ToolRegistry::new();

    // Register built-in tools
    tool_registry.register(Arc::new(tools::BashTool));
    tool_registry.register(Arc::new(tools::ReadTool));
    tool_registry.register(Arc::new(tools::WriteTool));
    tool_registry.register(Arc::new(tools::EditTool));
    tool_registry.register(Arc::new(tools::GlobTool));
    tool_registry.register(Arc::new(tools::GrepTool));

    // Register additional tools from core
    use yowcode_core::tools::*;
    tool_registry.register(Arc::new(CommitTool));
    tool_registry.register(Arc::new(DiffTool));
    tool_registry.register(Arc::new(AskUserQuestionTool));
    tool_registry.register(Arc::new(SleepTool));
    tool_registry.register(Arc::new(SyntheticOutputTool));
    tool_registry.register(Arc::new(GitStatusTool));
    tool_registry.register(Arc::new(GitBranchTool));
    tool_registry.register(Arc::new(ListDirectoryTool));
    tool_registry.register(Arc::new(FileInfoTool));
    tool_registry.register(Arc::new(WebFetchTool));
    tool_registry.register(Arc::new(WebSearchTool));

    let tool_registry = Arc::new(tool_registry);

    // Create command registry and register commands
    let command_registry = CommandRegistry::new();
    register_default_commands(&command_registry).await;

    let command_registry = Arc::new(command_registry);

    // Create broadcast channel for events
    let (tx, _rx) = broadcast::channel(1000);

    // Create executor
    let executor = Arc::new(ChatExecutor::new(
        ai_client,
        tool_registry.clone(),
        tx,
        config.ai.model.clone(),
    ));

    // Create a new session
    let permission_mode = if args.yolo {
        PermissionMode::Auto
    } else {
        PermissionMode::Default
    };

    let session = Session::new("CLI Session")
        .with_settings(SessionSettings {
            permission_mode,
            max_context_tokens: config.ai.max_tokens,
            theme: Some(config.cli.theme.clone()),
            project_id: None,
            interface_mode: yowcode_core::types::InterfaceMode::CLI,
        })
        .with_current_directory(
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        );

    let session_id = session_manager
        .create_session(session)
        .await
        .context("Failed to create session")?;

    // Create app
    let mut app = App::new(session_id, session_manager, executor, tool_registry, command_registry, config);

    // Load chat history
    app.load_history().await;

    // Run TUI
    run_tui(app).await?;

    info!("YowCode CLI exiting");

    Ok(())
}
