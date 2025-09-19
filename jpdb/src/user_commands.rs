use std::collections::HashMap;

/// All available commands in the jpdb debugger
#[derive(Debug, Clone, Copy)]
pub enum UserCommand {
    Quit,
    Next,
    Step,
    Help,
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
        }
    }

    /// Get the primary name of the command
    pub fn name(&self) -> &'static str {
        match self {
            UserCommand::Quit => "quit",
            UserCommand::Next => "next",
            UserCommand::Step => "step",
            UserCommand::Help => "help",
        }
    }

    /// Get all aliases for this command (including the primary name)
    pub fn aliases(&self) -> &'static [&'static str] {
        match self {
            UserCommand::Quit => &["quit", "q"],
            UserCommand::Next => &["next", "n", " "],
            UserCommand::Step => &["step", "s"],
            UserCommand::Help => &["help", "h"],
        }
    }

    /// Get a brief description for help listings
    pub fn description(&self) -> &'static str {
        match self {
            UserCommand::Quit => "Exit the debugger",
            UserCommand::Next => "Execute the next instruction",
            UserCommand::Step => "Step one instruction (same as next)",
            UserCommand::Help => "Show help information",
        }
    }

    /// Get detailed usage information
    pub fn usage(&self) -> &'static str {
        match self {
            UserCommand::Quit => "quit",
            UserCommand::Next => "next",
            UserCommand::Step => "step",
            UserCommand::Help => "help [command]",
        }
    }

    /// Get usage examples
    pub fn examples(&self) -> &'static [&'static str] {
        match self {
            UserCommand::Quit => &["quit", "q"],
            UserCommand::Next => &["next", "n", " "],
            UserCommand::Step => &["step", "s"],
            UserCommand::Help => &["help", "help next", "h quit"],
        }
    }

    /// Get all available commands
    pub fn all() -> &'static [UserCommand] {
        &[
            UserCommand::Quit,
            UserCommand::Next,
            UserCommand::Step,
            UserCommand::Help,
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

