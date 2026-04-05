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
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
    layout::Position,
};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::info;
use uuid::Uuid;
use yowcode_core::{
    ai::{AIConfig, AIProvider, OpenAICompatClient},
    commands::{self, CommandContext, CommandRegistry, parse_command, execute_command},
    config::Config,
    database::initialize_database,
    executor::{ChatExecutor, ExecutionEvent, ExecutionOptions},
    message::{ChatHistory, Message, MessageContent, MessageRole},
    session::{Session, SessionManager, SessionSettings},
    tool::ToolRegistry,
    types::PermissionMode,
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
}

/// Main application state
struct App {
    session_id: Uuid,
    messages: Vec<(MessageRole, String)>,
    input: String,
    input_mode: InputMode,
    current_directory: PathBuf,
    session_manager: Arc<SessionManager>,
    executor: Arc<ChatExecutor>,
    tool_registry: Arc<ToolRegistry>,
    command_registry: Arc<CommandRegistry>,
    status: String,
    is_thinking: bool,
}

/// Input mode for the application
#[derive(PartialEq)]
enum InputMode {
    Normal,
    Editing,
}

impl App {
    fn new(
        session_id: Uuid,
        session_manager: Arc<SessionManager>,
        executor: Arc<ChatExecutor>,
        tool_registry: Arc<ToolRegistry>,
        command_registry: Arc<CommandRegistry>,
    ) -> Self {
        Self {
            session_id,
            messages: Vec::new(),
            input: String::new(),
            input_mode: InputMode::Normal,
            current_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            session_manager,
            executor,
            tool_registry,
            command_registry,
            status: "Ready".to_string(),
            is_thinking: false,
        }
    }

    fn add_message(&mut self, role: MessageRole, content: String) {
        self.messages.push((role, content));
    }

    async fn send_message(&mut self) -> Result<()> {
        if self.input.is_empty() {
            return Ok(());
        }

        let query = self.input.clone();
        self.add_message(MessageRole::User, query.clone());
        self.input.clear();
        self.input_mode = InputMode::Normal;

        // Check if this is a command
        if let Some((cmd_name, _cmd_args)) = parse_command(&query) {
            self.status = format!("Executing command /{}...", cmd_name);

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
                    self.status = "Command error".to_string();
                    self.add_message(MessageRole::System, format!("Command error: {}", e));
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
    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('i') => {
                            app.input_mode = InputMode::Editing;
                        }
                        KeyCode::Char('q') => {
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                LeaveAlternateScreen,
                                DisableMouseCapture
                            )?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                LeaveAlternateScreen,
                                DisableMouseCapture
                            )?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            app.send_message().await?;
                        }
                        KeyCode::Char(c) => {
                            app.input.push(c);
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

/// UI rendering
fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(0), // Messages
                Constraint::Length(3), // Input
                Constraint::Length(1), // Status
            ]
            .as_ref(),
        )
        .split(f.area());

    // Render messages
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|(role, content)| {
            let (prefix, color) = match role {
                MessageRole::User => ("User", Color::Blue),
                MessageRole::Assistant => ("AI", Color::Green),
                MessageRole::System => ("System", Color::Yellow),
                MessageRole::Tool => ("Tool", Color::Cyan),
            };

            let lines: Vec<Line> = content
                .lines()
                .map(|line| {
                    Line::from(vec![
                        Span::styled(format!("[{}] ", prefix), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                        Span::styled(line, Style::default()),
                    ])
                })
                .collect();

            ListItem::new(Text::from(lines))
        })
        .collect();

    let messages_list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Chat"));

    f.render_widget(messages_list, chunks[0]);

    // Render input
    let input = Paragraph::new(app.input.as_str())
        .style(match app.input_mode {
            InputMode::Editing => Style::default().fg(Color::Yellow),
            InputMode::Normal => Style::default(),
        })
        .block(
            Block::default().borders(Borders::ALL)
                .title(if app.input_mode == InputMode::Editing {
                    "Input (Press Enter to send, Esc to cancel)"
                } else {
                    "Input (Press 'i' to edit, 'q' to quit)"
                }),
        );

    f.render_widget(input, chunks[1]);

    // Render status
    let status_text = if app.is_thinking {
        format!("🤔 {} | Press Ctrl+C to quit", app.status)
    } else {
        format!("{} | Press 'i' to input, 'q' to quit", app.status)
    };

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);

    f.render_widget(status, chunks[2]);

    // Show cursor in input mode
    if app.input_mode == InputMode::Editing {
        f.set_cursor_position(Position::new(
            chunks[1].x + app.input.len() as u16 + 1,
            chunks[1].y + 1,
        ));
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

    // Load configuration
    let config = Config::load(None).await.context("Failed to load configuration")?;

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

    let tool_registry = Arc::new(tool_registry);

    // Create command registry and register commands
    let command_registry = CommandRegistry::new();
    command_registry.register(Arc::new(commands::HelpCommand)).await;
    command_registry.register(Arc::new(commands::DiffCommand)).await;
    command_registry.register(Arc::new(commands::StatusCommand)).await;
    command_registry.register(Arc::new(commands::CommitCommand)).await;
    command_registry.register(Arc::new(commands::LsCommand)).await;
    command_registry.register(Arc::new(commands::CdCommand)).await;
    command_registry.register(Arc::new(commands::PwdCommand)).await;
    command_registry.register(Arc::new(commands::ClearCommand)).await;
    command_registry.register(Arc::new(commands::ToolsCommand)).await;

    let command_registry = Arc::new(command_registry);

    // Create broadcast channel for events
    let (tx, _rx) = broadcast::channel(1000);

    // Create executor
    let executor = Arc::new(ChatExecutor::new(ai_client, tool_registry.clone(), tx));

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
    let app = App::new(session_id, session_manager, executor, tool_registry, command_registry);

    // Run TUI
    run_tui(app).await?;

    info!("YowCode CLI exiting");

    Ok(())
}
