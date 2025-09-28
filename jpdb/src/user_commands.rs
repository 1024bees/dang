use std::collections::HashMap;

/// All available commands in the jpdb debugger
#[derive(Debug, Clone, Copy)]
pub enum UserCommand {
    Quit,
    Next,
    Step,
    Help,
    Clear,
    Breakpoint,
    Continue,
    Toggle,
}

impl UserCommand {
    /// Execute the command with the given application context
    pub fn execute(&self, app: &mut crate::App, args: &str) -> Result<(), String> {
        match self {
            UserCommand::Quit => {
                app.should_quit = true;
                Ok(())
            }
            UserCommand::Next => {
                app.add_execution_info();
                app.step_next();
                Ok(())
            }
            UserCommand::Step => {
                app.add_execution_info();
                app.step_next();
                Ok(())
            }
            UserCommand::Help => {
                let registry = CommandRegistry::new();
                if args.trim().is_empty() {
                    // Show all commands in LLDB style
                    app.command_history.push("Current command abbreviations (type 'help command alias' for more info):".to_string());

                    for cmd in UserCommand::all() {
                        let aliases_str = cmd.aliases().join(", ");
                        app.command_history.push(format!(
                            "  {:<9} -- {}",
                            aliases_str,
                            cmd.description()
                        ));
                    }

                    app.command_history.push("".to_string());
                    app.command_history.push("Keyboard shortcuts:".to_string());
                    app.command_history.push("  d         -- Toggle debug panel".to_string());
                    app.command_history.push("  Ctrl+D    -- Quit the debugger".to_string());
                    app.command_history.push("  Ctrl+L    -- Clear screen".to_string());
                    app.command_history.push("".to_string());
                } else {
                    // Show specific command help
                    let command_name = args.trim();
                    if let Some(command) = registry.get_command(command_name) {
                        app.command_history
                            .push(format!("Help for '{}':", command.name()));
                        app.command_history.push("".to_string());
                        app.command_history
                            .push(format!("Description: {}", command.description()));
                        app.command_history
                            .push(format!("Usage: {}", command.usage()));
                        app.command_history
                            .push(format!("Aliases: {}", command.aliases().join(", ")));
                        app.command_history.push("".to_string());
                        app.command_history.push("Examples:".to_string());
                        for example in command.examples() {
                            app.command_history.push(format!("  {example}"));
                        }
                    } else {
                        return Err(format!("Unknown command: {command_name}"));
                    }
                }
                Ok(())
            }
            UserCommand::Clear => {
                app.command_history.clear();
                app.scroll_offset = 0;
                Ok(())
            }
            UserCommand::Breakpoint => {
                let addr_str = args.trim();
                if addr_str.is_empty() {
                    return Err("breakpoint requires an address argument".to_string());
                }

                // Parse address (support both hex with 0x prefix and without)
                let addr = if addr_str.starts_with("0x") || addr_str.starts_with("0X") {
                    u32::from_str_radix(&addr_str[2..], 16)
                } else {
                    u32::from_str_radix(addr_str, 16)
                };

                match addr {
                    Ok(address) => {
                        match app.shucks_client.set_breakpoint(address) {
                            Ok(()) => {
                                app.command_history.push(format!("Breakpoint set at address 0x{:x}", address));
                                Ok(())
                            }
                            Err(e) => {
                                Err(format!("Failed to set breakpoint: {}", e))
                            }
                        }
                    }
                    Err(_) => {
                        Err(format!("Invalid address format: {}", addr_str))
                    }
                }
            }
            UserCommand::Continue => {
                app.command_history.push("Continuing...".to_string());
                // Send continue command via shucks client
                if let Err(e) = app.shucks_client.send_command_parsed(
                    shucks::Packet::Command(shucks::commands::GdbCommand::Resume(
                        shucks::commands::Resume::Continue
                    ))
                ) {
                    return Err(format!("Error continuing execution: {}", e));
                }
                Ok(())
            }
            UserCommand::Toggle => {
                app.show_split_view = !app.show_split_view;
                if app.show_split_view {
                    app.command_history.push("Split view enabled (instructions | source code)".to_string());
                } else {
                    app.command_history.push("Split view disabled".to_string());
                }
                Ok(())
            }
        }
    }

    /// Get the primary name of the command
    pub fn name(&self) -> &'static str {
        match self {
            UserCommand::Quit => "quit",
            UserCommand::Next => "next",
            UserCommand::Step => "step",
            UserCommand::Help => "help",
            UserCommand::Clear => "clear",
            UserCommand::Breakpoint => "breakpoint",
            UserCommand::Continue => "continue",
            UserCommand::Toggle => "toggle",
        }
    }

    /// Get all aliases for this command (including the primary name)
    pub fn aliases(&self) -> &'static [&'static str] {
        match self {
            UserCommand::Quit => &["quit", "q"],
            UserCommand::Next => &["next", "n", " "],
            UserCommand::Step => &["step", "s"],
            UserCommand::Help => &["help", "h"],
            UserCommand::Clear => &["clear", "cl"],
            UserCommand::Breakpoint => &["breakpoint", "b"],
            UserCommand::Continue => &["continue", "c"],
            UserCommand::Toggle => &["toggle", "t"],
        }
    }

    /// Get a brief description for help listings
    pub fn description(&self) -> &'static str {
        match self {
            UserCommand::Quit => "Exit the debugger",
            UserCommand::Next => "Execute the next instruction",
            UserCommand::Step => "Step one instruction (same as next)",
            UserCommand::Help => "Show help information",
            UserCommand::Clear => "Clear the screen",
            UserCommand::Breakpoint => "Set a breakpoint at the specified address",
            UserCommand::Continue => "Continue execution until breakpoint",
            UserCommand::Toggle => "Toggle split view (instructions | source code)",
        }
    }

    /// Get detailed usage information
    pub fn usage(&self) -> &'static str {
        match self {
            UserCommand::Quit => "quit",
            UserCommand::Next => "next",
            UserCommand::Step => "step",
            UserCommand::Help => "help [command]",
            UserCommand::Clear => "clear",
            UserCommand::Breakpoint => "breakpoint <address>",
            UserCommand::Continue => "continue",
            UserCommand::Toggle => "toggle",
        }
    }

    /// Get usage examples
    pub fn examples(&self) -> &'static [&'static str] {
        match self {
            UserCommand::Quit => &["quit", "q"],
            UserCommand::Next => &["next", "n", " "],
            UserCommand::Step => &["step", "s"],
            UserCommand::Help => &["help", "help next", "h quit"],
            UserCommand::Clear => &["clear", "cl"],
            UserCommand::Breakpoint => &["breakpoint 0x1000", "b 1000", "b 0x8000"],
            UserCommand::Continue => &["continue", "c"],
            UserCommand::Toggle => &["toggle", "t"],
        }
    }

    /// Get all available commands
    pub fn all() -> &'static [UserCommand] {
        &[
            UserCommand::Quit,
            UserCommand::Next,
            UserCommand::Step,
            UserCommand::Help,
            UserCommand::Clear,
            UserCommand::Breakpoint,
            UserCommand::Continue,
            UserCommand::Toggle,
        ]
    }
}

/// Registry that holds all available commands and handles lookup
pub struct CommandRegistry {
    alias_map: HashMap<String, UserCommand>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut alias_map = HashMap::new();

        // Build alias map for all commands
        for &command in UserCommand::all() {
            for alias in command.aliases() {
                alias_map.insert(alias.to_string(), command);
            }
        }

        Self { alias_map }
    }

    /// Get a command by name or alias
    pub fn get_command(&self, name: &str) -> Option<UserCommand> {
        self.alias_map.get(name).copied()
    }

    pub fn execute_command(
        &self,
        name: &str,
        args: &str,
        app: &mut crate::App,
    ) -> Result<(), String> {
        if let Some(command) = self.get_command(name) {
            command.execute(app, args)
        } else {
            Err(format!("Unknown command: {name}"))
        }
    }
}

