use std::{
    collections::VecDeque,
    io,
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

mod controller;
mod model;
mod user_commands;
mod view;

use controller::Controller;
use model::DebuggerModel;
use user_commands::CommandRegistry;
use view::ViewState;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Scrollbar},
    Frame, Terminal,
};
use shucks::{Client, Var};

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

pub struct AddSigState {
    active: bool,
    input: String,
    matches: Vec<(Var, String)>,
    selected_index: usize,
}

impl AddSigState {
    pub fn new() -> Self {
        Self {
            active: false,
            input: String::new(),
            matches: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn activate(&mut self) {
        self.active = true;
        self.input.clear();
        self.matches.clear();
        self.selected_index = 0;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.input.clear();
        self.matches.clear();
        self.selected_index = 0;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn update_search(&mut self, input: String) {
        self.input = input;
        self.selected_index = 0; // Reset selection when search changes
    }

    pub fn get_input(&self) -> &str {
        &self.input
    }

    pub fn set_matches(&mut self, matches: Vec<(Var, String)>) {
        self.matches = matches.into_iter().take(10).collect(); // Take top 10
        self.selected_index = self
            .selected_index
            .min(self.matches.len().saturating_sub(1));
    }

    pub fn get_matches(&self) -> &[(Var, String)] {
        &self.matches
    }

    pub fn select_next(&mut self) {
        if !self.matches.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.matches.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.matches.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.matches.len() - 1
            } else {
                self.selected_index - 1
            };
        }
    }

    pub fn get_selected(&self) -> Option<&(Var, String)> {
        self.matches.get(self.selected_index)
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected_index
    }
}

pub struct HelpModalState {
    active: bool,
    content: Vec<String>,
    scroll_offset: usize,
}

impl HelpModalState {
    pub fn new() -> Self {
        Self {
            active: false,
            content: Vec::new(),
            scroll_offset: 0,
        }
    }

    pub fn activate(&mut self, content: Vec<String>) {
        self.active = true;
        self.content = content;
        self.scroll_offset = 0;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.content.clear();
        self.scroll_offset = 0;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn get_content(&self) -> &[String] {
        &self.content
    }

    pub fn get_scroll_offset(&self) -> usize {
        self.scroll_offset
    }
}

pub struct App {
    pub should_quit: bool,
    input_buffer: String,
    pub command_history: Vec<String>,
    controller: Controller,
    view_state: ViewState,
    _dang_thread_handle: thread::JoinHandle<()>,
    scroll_offset: usize,
    // Debug panel state
    show_debug_panel: bool,
    debug_scroll_offset: usize, // Add scroll offset for debug panel
    // Split view state
    show_split_view: bool,
    log_buffer: Arc<Mutex<VecDeque<LogMessage>>>,
    // Last executed command for repeat functionality
    last_command: Option<String>,
    // Command history navigation
    user_command_history: Vec<String>,
    history_index: Option<usize>,
    // Addsig floating window state
    addsig_state: AddSigState,
    // Help modal state
    help_modal_state: HelpModalState,
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
        let workspace_root = std::env::current_dir()
            .expect("Failed to get current dir")
            .parent()
            .expect("Failed to get parent dir")
            .to_path_buf();
        let wave_path = workspace_root.join("test_data/ibex/sim.fst");

        shucks_client.initialize_gdb_session().expect("");
        let _ = shucks_client.load_elf_info();
        shucks_client
            .load_waveform(wave_path)
            .expect("Failed to load waveform");
        thread::sleep(Duration::from_millis(300));

        let mut controller = Controller::new(DebuggerModel::new(shucks_client));
        let mut view_state = ViewState::default();
        controller.refresh_all_views(&mut view_state);

        let app = App {
            should_quit: false,
            input_buffer: String::new(),
            command_history: Vec::new(),
            controller,
            view_state,
            _dang_thread_handle: dang_handle,
            scroll_offset: 0,
            show_debug_panel: false,
            debug_scroll_offset: 0, // Initialize debug scroll offset
            show_split_view: true,
            log_buffer,
            last_command: None,
            user_command_history: Vec::new(),
            history_index: None,
            addsig_state: AddSigState::new(),
            help_modal_state: HelpModalState::new(),
        };

        app
    }
}

impl App {
    fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        loop {
            terminal.draw(|f| self.ui(f))?;

            if let Event::Key(key) = event::read()? {
                // Check if we're in help modal mode first
                if self.help_modal_state.is_active() {
                    match key.code {
                        KeyCode::Up => {
                            self.help_modal_state.scroll_up(1);
                        }
                        KeyCode::Down => {
                            self.help_modal_state.scroll_down(1);
                        }
                        KeyCode::PageUp => {
                            self.help_modal_state.scroll_up(5);
                        }
                        KeyCode::PageDown => {
                            self.help_modal_state.scroll_down(5);
                        }
                        KeyCode::Home => {
                            // Scroll to top
                            let content_len = self.help_modal_state.get_content().len();
                            self.help_modal_state.scroll_up(content_len);
                        }
                        KeyCode::End => {
                            // Scroll to bottom
                            self.help_modal_state.scroll_down(usize::MAX);
                        }
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                            // Close help modal
                            self.help_modal_state.deactivate();
                        }
                        _ => {} // Ignore other keys in help modal mode
                    }
                } else if self.addsig_state.is_active() {
                    // Check if we're in addsig mode
                    match key.code {
                        KeyCode::Char(c) => {
                            // Add character to search input
                            let mut new_input = self.addsig_state.get_input().to_string();
                            new_input.push(c);
                            self.addsig_state.update_search(new_input);

                            // Update fuzzy matches via controller
                            let matches = self
                                .controller
                                .fuzzy_match_signals(self.addsig_state.get_input());
                            self.addsig_state.set_matches(matches);
                        }
                        KeyCode::Backspace => {
                            // Remove character from search input
                            let mut new_input = self.addsig_state.get_input().to_string();
                            new_input.pop();
                            self.addsig_state.update_search(new_input);

                            // Update fuzzy matches via controller
                            let matches = self
                                .controller
                                .fuzzy_match_signals(self.addsig_state.get_input());
                            self.addsig_state.set_matches(matches);
                        }
                        KeyCode::Up => {
                            self.addsig_state.select_prev();
                        }
                        KeyCode::Down => {
                            self.addsig_state.select_next();
                        }
                        KeyCode::Enter => {
                            // Select the signal and exit addsig mode
                            if let Some((var, _)) = self.addsig_state.get_selected().cloned() {
                                self.controller.select_signal(var);
                                self.controller.refresh_signal_view(&mut self.view_state);
                            }
                            self.addsig_state.deactivate();
                        }
                        KeyCode::Esc => {
                            // Exit addsig mode without selection
                            self.addsig_state.deactivate();
                        }
                        _ => {} // Ignore other keys in addsig mode
                    }
                } else {
                    // Normal key handling when not in addsig mode
                    match key.code {
                        KeyCode::Char('d')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            // Ctrl+D: Quit the application
                            self.should_quit = true;
                        }
                        KeyCode::Char('l')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            // Ctrl+L: Clear screen
                            self.command_history.clear();
                            self.scroll_offset = 0;
                        }

                        // Debug panel scrolling (only when debug panel is visible)
                        KeyCode::PageUp if self.show_debug_panel => {
                            // Scroll up in debug panel
                            self.debug_scroll_offset = self.debug_scroll_offset.saturating_add(5);
                        }
                        KeyCode::PageDown if self.show_debug_panel => {
                            // Scroll down in debug panel
                            self.debug_scroll_offset = self.debug_scroll_offset.saturating_sub(5);
                        }
                        KeyCode::Home if self.show_debug_panel => {
                            // Go to top of debug panel
                            if let Ok(buffer) = self.log_buffer.lock() {
                                self.debug_scroll_offset = buffer.len().saturating_sub(1);
                            }
                        }
                        KeyCode::End if self.show_debug_panel => {
                            // Go to bottom of debug panel
                            self.debug_scroll_offset = 0;
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
                                            self.input_buffer =
                                                self.user_command_history[0].clone();
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    pub fn step_next(&mut self) {
        if let Err(e) = self.controller.step() {
            self.command_history.push(format!("Error stepping: {e}"));
            return;
        }

        self.refresh_all_views();
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

    fn refresh_all_views(&mut self) {
        self.controller.refresh_all_views(&mut self.view_state);
    }

    pub fn set_breakpoint(&mut self, address: u32) -> Result<(), String> {
        self.controller.set_breakpoint(address)
    }

    pub fn set_breakpoint_at_line(&mut self, file: &str, line: u64) -> Result<Vec<u32>, String> {
        self.controller.set_breakpoint_at_line(file, line)
    }

    pub fn continue_execution(&mut self) -> Result<(), String> {
        self.controller.continue_execution()
    }

    pub fn invalidate_time_idx_cache(&mut self) {
        self.controller.invalidate_time_index();
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

        // Render addsig popup on top if active
        if self.addsig_state.is_active() {
            self.render_addsig_popup(f, f.area());
        }

        // Render help modal on top if active
        if self.help_modal_state.is_active() {
            self.render_help_modal(f, f.area());
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
        let items: Vec<ListItem> = self
            .view_state
            .execution_lines
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

    fn render_command_input(
        &self,
        f: &mut Frame,
        area: ratatui::layout::Rect,
        show_full_history: bool,
        history_lines: usize,
    ) {
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

        let title = if show_full_history {
            "Command History"
        } else {
            "Command"
        };
        let command_area =
            List::new(items).block(Block::default().borders(Borders::ALL).title(title));

        f.render_widget(command_area, area);
    }

    fn render_command_area(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Use shared component with full history display
        self.render_command_input(f, area, true, 0);
    }

    fn render_debug_panel(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Get all log messages from buffer
        let all_log_messages = if let Ok(buffer) = self.log_buffer.lock() {
            buffer.iter().cloned().collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let available_height = area.height.saturating_sub(2) as usize; // Account for borders
        let total_messages = all_log_messages.len();

        // Calculate which messages to show based on scroll offset
        let visible_messages = if total_messages > available_height {
            let max_scroll = total_messages.saturating_sub(available_height);
            let actual_scroll = self.debug_scroll_offset.min(max_scroll);
            let start_idx = total_messages.saturating_sub(available_height + actual_scroll);
            let end_idx = start_idx + available_height;

            all_log_messages[start_idx..end_idx.min(total_messages)].to_vec()
        } else {
            // If all messages fit, show them all
            all_log_messages
        };

        let items: Vec<ListItem> = visible_messages
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
                .title("Debug (d to toggle, PgUp/PgDn to scroll, Home/End)"),
        );

        f.render_widget(debug_panel, area);

        // Add scrollbar if there are more messages than can fit
        if total_messages > available_height {
            let scrollbar_area = Rect {
                x: area.x + area.width - 1,
                y: area.y + 1,
                width: 1,
                height: area.height - 2,
            };

            let max_scroll = total_messages.saturating_sub(available_height);
            let scrollbar = Scrollbar::default()
                .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(total_messages)
                .position(
                    total_messages.saturating_sub(
                        available_height + self.debug_scroll_offset.min(max_scroll),
                    ),
                );

            f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }

    fn render_split_view(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Split the area vertically: panels (top 70%) and command bar (bottom 30%)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
            .split(area);

        // Split the top area horizontally: instructions (left), source code (middle), signals (right)
        let panel_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(30),
                    Constraint::Percentage(30),
                    Constraint::Percentage(40),
                ]
                .as_ref(),
            )
            .split(main_chunks[0]);

        self.render_instruction_pane(f, panel_chunks[0]);
        self.render_source_pane(f, panel_chunks[1]);
        self.render_signal_panel(f, panel_chunks[2]);
        self.render_command_bar(f, main_chunks[1]);
    }

    fn render_instruction_pane(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .view_state
            .instruction_lines
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
        let items: Vec<ListItem> = self
            .view_state
            .source_lines
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

    fn render_signal_panel(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .view_state
            .signal_lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let style = if i == 0 && line.ends_with(" ps") {
                    // Time header - make it bold and colored
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("Error:") || line.starts_with("Error ") {
                    Style::default().fg(Color::Red)
                } else if line == "no waves found" || line == "No signals selected" {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(line.clone()).style(style)
            })
            .collect();

        let signal_panel = List::new(items).block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title("Signals"),
        );

        f.render_widget(signal_panel, area);
    }

    fn render_command_bar(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Use shared component with compact history (show last 3 commands)
        self.render_command_input(f, area, false, 3);
    }

    fn render_addsig_popup(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::{Clear, Paragraph};

        // Calculate popup size and position (centered, 60% width, 50% height)
        let popup_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(25), // Top margin
                Constraint::Percentage(50), // Popup height
                Constraint::Percentage(25), // Bottom margin
            ])
            .split(area)[1];

        let popup_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20), // Left margin
                Constraint::Percentage(60), // Popup width
                Constraint::Percentage(20), // Right margin
            ])
            .split(popup_area)[1];

        // Clear the background
        f.render_widget(Clear, popup_area);

        // Split popup into search input and results
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Min(0),    // Results
            ])
            .split(popup_area);

        // Render search input
        let input_text = format!("Search: {}", self.addsig_state.get_input());
        let input_paragraph = Paragraph::new(input_text)
            .block(Block::default().borders(Borders::ALL).title("Add Signal"))
            .alignment(Alignment::Left);
        f.render_widget(input_paragraph, chunks[0]);

        // Render search results
        let matches = self.addsig_state.get_matches();
        let selected_index = self.addsig_state.get_selected_index();

        let items: Vec<ListItem> = matches
            .iter()
            .enumerate()
            .map(|(i, (_, signal_name))| {
                let style = if i == selected_index {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(signal_name.clone()).style(style)
            })
            .collect();

        let results_list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Signals"))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(results_list, chunks[1]);

        // Add help text at the bottom
        let help_area = Rect {
            x: popup_area.x,
            y: popup_area.y + popup_area.height,
            width: popup_area.width,
            height: 1,
        };
        let help_text = Paragraph::new("↑↓: Navigate | Enter: Select | Esc: Cancel")
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        f.render_widget(help_text, help_area);
    }

    fn render_help_modal(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::{Clear, Paragraph};

        // Calculate popup size and position (centered, 70% width, 60% height)
        let popup_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(20), // Top margin
                Constraint::Percentage(60), // Popup height
                Constraint::Percentage(20), // Bottom margin
            ])
            .split(area)[1];

        let popup_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(15), // Left margin
                Constraint::Percentage(70), // Popup width
                Constraint::Percentage(15), // Right margin
            ])
            .split(popup_area)[1];

        // Clear the background
        f.render_widget(Clear, popup_area);

        // Get content and calculate visible area
        let content = self.help_modal_state.get_content();
        let available_height = popup_area.height.saturating_sub(2) as usize; // Account for borders
        let total_lines = content.len();
        let scroll_offset = self.help_modal_state.get_scroll_offset();

        // Calculate which lines to show based on scroll offset
        let visible_content = if total_lines > available_height {
            let max_scroll = total_lines.saturating_sub(available_height);
            let actual_scroll = scroll_offset.min(max_scroll);
            let start_idx = total_lines.saturating_sub(available_height + actual_scroll);
            let end_idx = start_idx + available_height;

            &content[start_idx..end_idx.min(total_lines)]
        } else {
            content
        };

        // Render help content
        let items: Vec<ListItem> = visible_content
            .iter()
            .map(|line| {
                let style = if line.starts_with("Current command") || line.starts_with("Help for") {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("  ") && line.contains("--") {
                    // Command line
                    Style::default().fg(Color::Yellow)
                } else if line.starts_with("Keyboard shortcuts:")
                    || line.starts_with("Description:")
                    || line.starts_with("Usage:")
                    || line.starts_with("Aliases:")
                    || line.starts_with("Examples:")
                {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(line.clone()).style(style)
            })
            .collect();

        let help_list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help (Press Esc, Enter, or 'q' to close)"),
        );

        f.render_widget(help_list, popup_area);

        // Add scrollbar if there's more content than can fit
        if total_lines > available_height {
            let scrollbar_area = Rect {
                x: popup_area.x + popup_area.width - 1,
                y: popup_area.y + 1,
                width: 1,
                height: popup_area.height - 2,
            };

            let max_scroll = total_lines.saturating_sub(available_height);
            let scrollbar = Scrollbar::default()
                .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(total_lines).position(
                total_lines.saturating_sub(available_height + scroll_offset.min(max_scroll)),
            );

            f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }

        // Add navigation help text at the bottom
        let help_area = Rect {
            x: popup_area.x,
            y: popup_area.y + popup_area.height,
            width: popup_area.width,
            height: 1,
        };
        let nav_text = Paragraph::new(
            "↑↓: Scroll | PgUp/PgDn: Page | Home/End: Top/Bottom | Esc/Enter/q: Close",
        )
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
        f.render_widget(nav_text, help_area);
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
        log::error!("{err:?}");
    }

    Ok(())
}
