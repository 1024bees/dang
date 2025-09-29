use std::{
    collections::VecDeque,
    io,
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

mod user_commands;
use user_commands::CommandRegistry;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame, Terminal,
};
use shucks::{
    commands::{GdbCommand, Resume},
    Client, Packet,
};

// Custom logger that captures messages for ratatui display
#[derive(Debug, Clone)]
pub struct LogMessage {
    level: log::Level,
    message: String,
    _timestamp: std::time::Instant,
}

pub struct AppLogger {
    buffer: Arc<Mutex<VecDeque<LogMessage>>>,
}

impl AppLogger {
    pub fn new() -> (Self, Arc<Mutex<VecDeque<LogMessage>>>) {
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(1000)));
        (
            Self {
                buffer: buffer.clone(),
            },
            buffer,
        )
    }
}

impl log::Log for AppLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let message = LogMessage {
                level: record.level(),
                message: record.args().to_string(),
                _timestamp: std::time::Instant::now(),
            };

            if let Ok(mut buffer) = self.buffer.lock() {
                // Keep only the last 1000 log messages
                if buffer.len() >= 1000 {
                    buffer.pop_front();
                }
                buffer.push_back(message);
            }
        }
    }

    fn flush(&self) {}
}

pub struct App {
    pub should_quit: bool,

    input_buffer: String,
    pub command_history: Vec<String>,
    pub instruction_output: Vec<String>,
    pub shucks_client: Client,
    _dang_thread_handle: thread::JoinHandle<()>,
    scroll_offset: usize,
    // Debug panel state
    show_debug_panel: bool,
    // Split view state
    show_split_view: bool,
    log_buffer: Arc<Mutex<VecDeque<LogMessage>>>,
    // Last executed command for repeat functionality
    last_command: Option<String>,
    // Command history navigation
    user_command_history: Vec<String>,
    history_index: Option<usize>,
}

impl Default for App {
    fn default() -> App {
        // Initialize custom logging system
        let (logger, log_buffer) = AppLogger::new();
        log::set_boxed_logger(Box::new(logger))
            .map(|()| log::set_max_level(log::LevelFilter::Debug))
            .expect("Failed to initialize logger");

        // Create TCP listener for dang-shucks communication
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind listener");
        let port = listener
            .local_addr()
            .expect("Failed to get local addr")
            .port();

        // Start dang GDB stub in a separate thread
        let dang_handle = thread::spawn(move || {
            let workspace_root = std::env::current_dir()
                .expect("Failed to get current dir")
                .parent()
                .expect("Failed to get parent dir")
                .to_path_buf();

            let wave_path = workspace_root.join("test_data/ibex/sim.fst");
            let mapping_path = workspace_root.join("test_data/ibex/signal_get.py");
            let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

            dang::start_with_args_and_listener_silent(wave_path, mapping_path, elf_path, listener)
                .expect("Failed to start dang");
        });

        // Give dang time to start
        thread::sleep(std::time::Duration::from_millis(300));

        // Create shucks client connected to dang
        let mut shucks_client = Client::new_with_port(port);

        shucks_client.initialize_gdb_session().expect("");
        let _ = shucks_client.load_elf_info();

        thread::sleep(Duration::from_millis(300));

        let mut app = App {
            should_quit: false,
            input_buffer: String::new(),
            command_history: Vec::new(),
            instruction_output: Vec::new(),
            shucks_client,
            _dang_thread_handle: dang_handle,
            scroll_offset: 0,
            show_debug_panel: false,
            show_split_view: false,
            log_buffer,
            last_command: None,
            user_command_history: Vec::new(),
            history_index: None,
        };

        // Show initial instructions when first connecting
        app.add_execution_info();

        app
    }
}

impl App {
    fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        loop {
            terminal.draw(|f| self.ui(f))?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('d') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        // Ctrl+D: Quit the application
                        self.should_quit = true;
                    }
                    KeyCode::Char('l') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        // Ctrl+L: Clear screen
                        self.command_history.clear();
                        self.scroll_offset = 0;
                    }
                    KeyCode::Char('d') => {
                        // Toggle debug panel
                        self.show_debug_panel = !self.show_debug_panel;
                    }
                    KeyCode::Char(c) => {
                        self.input_buffer.push(c);
                        // Reset history navigation when user types
                        self.history_index = None;
                    }
                    KeyCode::Enter => {
                        self.process_command();
                        self.input_buffer.clear();
                        // Auto-scroll to bottom when new command is entered
                        self.scroll_offset = 0;
                    }
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                        // Reset history navigation when user modifies input
                        self.history_index = None;
                    }
                    KeyCode::Up => {
                        // Navigate to previous command in history
                        if !self.user_command_history.is_empty() {
                            let new_index = match self.history_index {
                                None => self.user_command_history.len() - 1,
                                Some(index) => {
                                    if index > 0 {
                                        index - 1
                                    } else {
                                        // Wrap to newest (end of history)
                                        self.user_command_history.len() - 1
                                    }
                                }
                            };
                            self.history_index = Some(new_index);
                            self.input_buffer = self.user_command_history[new_index].clone();
                        }
                    }
                    KeyCode::Down => {
                        // Navigate to next (more recent) command in history
                        if !self.user_command_history.is_empty() {
                            match self.history_index {
                                None => {
                                    // Do nothing if not currently navigating history
                                }
                                Some(index) => {
                                    if index < self.user_command_history.len() - 1 {
                                        let new_index = index + 1;
                                        self.history_index = Some(new_index);
                                        self.input_buffer =
                                            self.user_command_history[new_index].clone();
                                    } else {
                                        // Wrap to oldest (beginning of history)
                                        self.history_index = Some(0);
                                        self.input_buffer = self.user_command_history[0].clone();
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    pub fn step_next(&mut self) {
        // Send step command to shucks/dang instead of using hardcoded logic
        if let Err(e) = self
            .shucks_client
            .send_command_parsed(Packet::Command(GdbCommand::Resume(Resume::Step)))
        {
            self.command_history.push(format!("Error stepping: {e}"));
        }
    }

    fn process_command(&mut self) {
        let input = self.input_buffer.trim().to_string();

        // Handle empty input - repeat last command if available
        let command_to_execute = if input.is_empty() {
            if let Some(ref last_cmd) = self.last_command {
                // Show that we're repeating the last command
                let display_command = format!("(jpdb) {last_cmd}");
                self.command_history.push(display_command);
                last_cmd.clone()
            } else {
                // No last command to repeat
                return;
            }
        } else {
            // Store non-empty command as last command (but exclude certain commands)
            if !matches!(input.as_str(), "quit" | "q" | "clear" | "cl") {
                self.last_command = Some(input.clone());
            }

            // Add user command to history (exclude certain system commands)
            if !matches!(input.as_str(), "quit" | "q" | "clear" | "cl") {
                self.user_command_history.push(input.clone());
            }

            // Reset history navigation when a new command is entered
            self.history_index = None;

            // Add command to display history
            let display_command = format!("(jpdb) {input}");
            self.command_history.push(display_command);
            input
        };

        // Parse command and arguments
        let parts: Vec<&str> = command_to_execute.splitn(2, ' ').collect();
        let command_name = parts[0];
        let args = parts.get(1).map_or("", |v| *v);

        // Execute command using registry
        let registry = CommandRegistry::new();
        if let Err(error) = registry.execute_command(command_name, args, self) {
            self.command_history.push(format!("error: {error}"));
        }
    }

    pub fn add_execution_info(&mut self) {
        log::debug!("Adding execution info");
        self.instruction_output.clear(); // Clear previous instruction output
        self.instruction_output.push("Process 1 stopped".to_string());
        self.instruction_output
            .push("* thread #1, stop reason = instruction step over".to_string());

        // Get current PC from shucks
        log::debug!("About to get current PC");
        match self.shucks_client.get_current_pc() {
            Ok(current_pc) => {
                log::debug!("Successfully got current PC: 0x{current_pc}");
                let frame_info = format!("    frame #0: 0x{current_pc}");
                self.instruction_output.push(frame_info);

                // Try to get current instruction info from shucks

                if let Ok(insts) = self.shucks_client.get_current_and_next_inst() {
                    // Only show arrow for the first instruction (current PC)
                    for (i, ainst) in insts.iter().enumerate() {
                        let inst_pc = ainst.pc().as_u32();
                        if i == 0 {
                            // First instruction gets the arrow
                            self.instruction_output
                                .push(format!("->  0x{inst_pc:x}: {ainst}"));
                        } else {
                            // Subsequent instructions without arrow
                            self.instruction_output
                                .push(format!("    0x{inst_pc:x}: {ainst}"));
                        }
                    }
                } else {
                    self.instruction_output
                        .push(format!("->  0x{current_pc}: <unable to get instructions>"));
                }
            }
            Err(e) => {
                log::error!("Failed to get current PC: {e}");
                self.instruction_output.push(format!("Error getting PC: {e}"));
            }
        }

        self.instruction_output
            .push("Target 0: (No executable module.) stopped.".to_string());
    }

    fn ui(&mut self, f: &mut Frame) {
        if self.show_debug_panel {
            // Split the layout: main area (70%) and debug panel (30%)
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
                .split(f.area());

            if self.show_split_view {
                self.render_split_view(f, chunks[0]);
            } else {
                self.render_combined_output(f, chunks[0]);
            }
            self.render_debug_panel(f, chunks[1]);
        } else if self.show_split_view {
            // Show split view without debug panel
            self.render_split_view(f, f.area());
        } else {
            // Render everything as one continuous output with prompt at the end
            self.render_combined_output(f, f.area());
        }
    }

    fn render_combined_output(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Split the area vertically: instruction panel (top 40%) and command area (bottom 60%)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
            .split(area);

        // Render instruction panel at the top
        self.render_instruction_panel_combined(f, chunks[0]);

        // Render command history and prompt at the bottom
        self.render_command_area(f, chunks[1]);
    }

    fn render_instruction_panel_combined(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self.instruction_output
            .iter()
            .map(|line| {
                let style = if line.starts_with("->") {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("Error") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(line.clone()).style(style)
            })
            .collect();

        let instruction_panel = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Execution State"),
        );

        f.render_widget(instruction_panel, area);
    }

    fn render_command_input(&self, f: &mut Frame, area: ratatui::layout::Rect, show_full_history: bool, history_lines: usize) {
        let mut all_lines: Vec<String> = if show_full_history {
            // Show full command history for non-split view
            self.command_history.clone()
        } else {
            // Show only recent history for split view
            let start_idx = self.command_history.len().saturating_sub(history_lines);
            self.command_history[start_idx..].to_vec()
        };

        // Add the current prompt line
        let prompt_text = format!("(jpdb) {}", self.input_buffer);
        all_lines.push(prompt_text);

        // Calculate how many lines can fit in the terminal
        let available_height = area.height.saturating_sub(2) as usize; // Account for borders
        let total_lines = all_lines.len();

        // Determine which lines to show based on scroll offset (only for full history mode)
        let visible_lines = if show_full_history && total_lines > available_height {
            // Need to scroll - calculate the start index
            let max_scroll = total_lines.saturating_sub(available_height);
            let actual_scroll = self.scroll_offset.min(max_scroll);
            let start_idx = total_lines.saturating_sub(available_height + actual_scroll);
            let end_idx = start_idx + available_height;

            all_lines[start_idx..end_idx.min(total_lines)].to_vec()
        } else {
            // For split view or when all lines fit, show from the end
            let start_idx = total_lines.saturating_sub(available_height);
            all_lines[start_idx..].to_vec()
        };

        let items: Vec<ListItem> = visible_lines
            .iter()
            .map(|line| {
                let style = if line.starts_with("(jpdb)") {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("error:") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(line.clone()).style(style)
            })
            .collect();

        let title = if show_full_history { "Command History" } else { "Command" };
        let command_area = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title),
        );

        f.render_widget(command_area, area);
    }

    fn render_command_area(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Use shared component with full history display
        self.render_command_input(f, area, true, 0);
    }

    fn render_debug_panel(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Get log messages from buffer - clone them to avoid lifetime issues
        let log_messages = if let Ok(buffer) = self.log_buffer.lock() {
            buffer
                .iter()
                .rev()
                .take(area.height.saturating_sub(2) as usize)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let items: Vec<ListItem> = log_messages
            .iter()
            .map(|msg| {
                let style = match msg.level {
                    log::Level::Error => Style::default().fg(Color::Red),
                    log::Level::Warn => Style::default().fg(Color::Yellow),
                    log::Level::Info => Style::default().fg(Color::Blue),
                    log::Level::Debug => Style::default().fg(Color::Gray),
                    log::Level::Trace => Style::default().fg(Color::DarkGray),
                };
                let formatted_msg = format!("[{}] {}", msg.level, msg.message);
                ListItem::new(formatted_msg).style(style)
            })
            .collect();

        let debug_panel = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Debug (d to toggle)"),
        );

        f.render_widget(debug_panel, area);
    }

    fn render_split_view(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Split the area vertically: panels (top 70%) and command bar (bottom 30%)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
            .split(area);

        // Split the top area horizontally: instructions (left) and source code (right)
        let panel_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(main_chunks[0]);

        self.render_instruction_pane(f, panel_chunks[0]);
        self.render_source_pane(f, panel_chunks[1]);
        self.render_command_bar(f, main_chunks[1]);
    }

    fn render_instruction_pane(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Get current and next instructions
        let mut instruction_lines = Vec::new();

        match self.shucks_client.get_current_and_next_inst() {
            Ok(instructions) => {
                for (i, inst) in instructions.iter().enumerate() {
                    let pc = inst.pc().as_u32();
                    if i == 0 {
                        // Current instruction with arrow
                        instruction_lines.push(format!("->  0x{pc:x}: {inst}"));
                    } else {
                        // Next instructions
                        instruction_lines.push(format!("    0x{pc:x}: {inst}"));
                    }
                }
            }
            Err(e) => {
                instruction_lines.push(format!("Error: {e}"));
            }
        }

        let items: Vec<ListItem> = instruction_lines
            .iter()
            .map(|line| {
                let style = if line.starts_with("->") {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("Error:") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(line.clone()).style(style)
            })
            .collect();

        let instruction_panel = List::new(items).block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title("Instructions"),
        );

        f.render_widget(instruction_panel, area);
    }

    fn render_source_pane(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        let mut source_lines = Vec::new();

        // Get current source line
        match self.shucks_client.get_current_source_line() {
            Ok(Some(current_line)) => {
                source_lines.push(format!(
                    "{}:{}",
                    current_line
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown"),
                    current_line.line
                ));
                source_lines.push("".to_string());

                // Show current line with arrow
                if let Some(ref text) = current_line.text {
                    source_lines.push(format!("-> {}: {}", current_line.line, text));
                } else {
                    source_lines.push(format!("-> {}: <source not available>", current_line.line));
                }

                // Get next 3 consecutive source lines from the same file
                match self
                    .shucks_client
                    .get_consecutive_source_lines_after_current(3)
                {
                    Ok(next_lines) => {
                        for line in next_lines {
                            if let Some(ref text) = line.text {
                                source_lines.push(format!("   {}: {}", line.line, text));
                            } else {
                                source_lines
                                    .push(format!("   {}: <source not available>", line.line));
                            }
                        }
                    }
                    Err(e) => {
                        source_lines.push(format!("Error getting next lines: {e}"));
                    }
                }
            }
            Ok(None) => {
                source_lines.push("Source Code:".to_string());
                source_lines.push("No debug information available".to_string());
            }
            Err(e) => {
                source_lines.push("Source Code:".to_string());
                source_lines.push(format!("Error: {e}"));
            }
        }

        let items: Vec<ListItem> = source_lines
            .iter()
            .map(|line| {
                let style = if line.starts_with("->") {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("Error:") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(line.clone()).style(style)
            })
            .collect();

        let source_panel = List::new(items).block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title("Source Code"),
        );

        f.render_widget(source_panel, area);
    }

    fn render_command_bar(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Use shared component with compact history (show last 3 commands)
        self.render_command_input(f, area, false, 3);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();
    let res = app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}
