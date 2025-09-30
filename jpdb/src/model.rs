use shucks::{Client, TimeTableIdx, Var};

pub struct DebuggerModel {
    pub client: Client,
    cached_time_idx: Option<u64>,
    terminated: bool,
}

pub struct ExecutionSnapshot {
    pub summary_lines: Vec<String>,
    pub instruction_lines: Vec<String>,
}

pub struct SourceSnapshot {
    pub lines: Vec<String>,
}

pub struct SignalSnapshot {
    pub lines: Vec<String>,
}

pub type ModelResult<T> = Result<T, String>;

impl DebuggerModel {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            cached_time_idx: None,
            terminated: false,
        }
    }

    pub fn step(&mut self) -> ModelResult<()> {
        if self.terminated {
            return Err("Process has terminated".to_string());
        }

        let still_alive = self.client.step().map_err(|e| e.to_string())?;
        if !still_alive {
            self.terminated = true;
            return Err("Process has terminated".to_string());
        }

        self.invalidate_time_index();
        Ok(())
    }

    pub fn continue_execution(&mut self) -> ModelResult<()> {
        if self.terminated {
            return Err("Process has terminated".to_string());
        }

        let still_alive = self
            .client
            .continue_execution()
            .map_err(|e| e.to_string())?;
        if !still_alive {
            self.terminated = true;
            return Err("Process has terminated".to_string());
        }

        self.invalidate_time_index();
        Ok(())
    }

    pub fn set_breakpoint(&mut self, address: u32) -> ModelResult<()> {
        self.client
            .set_breakpoint(address)
            .map_err(|e| e.to_string())
    }

    pub fn set_breakpoint_at_line(&mut self, file: &str, line: u64) -> ModelResult<Vec<u32>> {
        self.client
            .set_breakpoint_at_line(file, line)
            .map_err(|e| e.to_string())
    }

    pub fn fetch_execution_snapshot(&mut self) -> ModelResult<ExecutionSnapshot> {
        if self.terminated {
            return Ok(ExecutionSnapshot {
                summary_lines: vec!["Process has terminated".to_string()],
                instruction_lines: vec!["Process has terminated".to_string()],
            });
        }

        let mut summary_lines = Vec::new();
        summary_lines.push("Process 1 stopped".to_string());
        summary_lines.push("* thread #1, stop reason = instruction step over".to_string());

        let mut instruction_lines = Vec::new();

        match self.client.get_current_pc() {
            Ok(current_pc) => {
                summary_lines.push(format!("    frame #0: 0x{current_pc}"));

                match self.client.get_current_and_next_inst() {
                    Ok(insts) => {
                        for (i, inst) in insts.iter().enumerate() {
                            let inst_pc = inst.pc().as_u32();
                            let formatted = if i == 0 {
                                format!("->  0x{inst_pc:x}: {inst}")
                            } else {
                                format!("    0x{inst_pc:x}: {inst}")
                            };
                            summary_lines.push(formatted.clone());
                            instruction_lines.push(formatted);
                        }
                    }
                    Err(_) => {
                        summary_lines
                            .push(format!("->  0x{current_pc}: <unable to get instructions>"));
                        instruction_lines
                            .push(format!("->  0x{current_pc}: <unable to get instructions>"));
                    }
                }
            }
            Err(e) => {
                summary_lines.push(format!("Error getting PC: {e}"));
                instruction_lines.push(format!("Error: {e}"));
            }
        }

        summary_lines.push("Target 0: (No executable module.) stopped.".to_string());

        Ok(ExecutionSnapshot {
            summary_lines,
            instruction_lines,
        })
    }

    pub fn fetch_source_snapshot(&mut self) -> ModelResult<SourceSnapshot> {
        if self.terminated {
            return Ok(SourceSnapshot {
                lines: vec!["Process has terminated".to_string()],
            });
        }

        let mut lines = Vec::new();

        match self.client.get_current_source_line() {
            Ok(Some(current_line)) => {
                lines.push(format!(
                    "{}:{}",
                    current_line
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown"),
                    current_line.line
                ));
                lines.push(String::new());

                if let Some(ref text) = current_line.text {
                    lines.push(format!("-> {}: {}", current_line.line, text));
                } else {
                    lines.push(format!("-> {}: <source not available>", current_line.line));
                }

                match self.client.get_consecutive_source_lines_after_current(3) {
                    Ok(next_lines) => {
                        for line in next_lines {
                            if let Some(ref text) = line.text {
                                lines.push(format!("   {}: {}", line.line, text));
                            } else {
                                lines.push(format!("   {}: <source not available>", line.line));
                            }
                        }
                    }
                    Err(e) => {
                        lines.push(format!("Error getting next lines: {e}"));
                    }
                }
            }
            Ok(None) => {
                lines.push("Source Code:".to_string());
                lines.push("No debug information available".to_string());
            }
            Err(e) => {
                lines.push("Source Code:".to_string());
                lines.push(format!("Error: {e}"));
            }
        }

        Ok(SourceSnapshot { lines })
    }

    pub fn fetch_signal_snapshot(&mut self) -> ModelResult<SignalSnapshot> {
        if self.terminated {
            return Ok(SignalSnapshot {
                lines: vec!["Process has terminated".to_string()],
            });
        }

        if self.client.wave_tracker.is_none() {
            return Ok(SignalSnapshot {
                lines: vec!["no waves found".to_string()],
            });
        }

        let time_idx = self.get_time_index()?;

        let mut lines = Vec::new();
        if let Some(ref mut tracker) = self.client.wave_tracker {
            let current_time = tracker.get_current_time(time_idx as TimeTableIdx);
            lines.push(format!("{current_time} ps"));
            lines.push(String::new());

            let signal_names = tracker.get_signal_names();
            if signal_names.is_empty() {
                lines.push("No signals selected".to_string());
                lines.push("Use 'addsig' to add signals".to_string());
            } else {
                let signal_values = tracker.get_values(time_idx as TimeTableIdx);
                for (name, value) in signal_names.iter().zip(signal_values.iter()) {
                    lines.push(format!("{name}: {value}"));
                }
            }
        }

        Ok(SignalSnapshot { lines })
    }

    pub fn fuzzy_match_signals(&mut self, query: &str) -> Vec<(Var, String)> {
        if let Some(ref mut tracker) = self.client.wave_tracker {
            tracker.fuzzy_match_var(query)
        } else {
            Vec::new()
        }
    }

    pub fn select_signal(&mut self, var: Var) {
        if let Some(ref mut tracker) = self.client.wave_tracker {
            tracker.select_signal(var);
        }
    }

    pub fn most_recent_var_path(&self) -> Option<String> {
        if let Some(ref tracker) = self.client.wave_tracker {
            tracker.get_signal_names().last().cloned()
        } else {
            None
        }
    }

    pub fn invalidate_time_index(&mut self) {
        self.cached_time_idx = None;
    }

    pub fn get_time_idx(&mut self) -> ModelResult<u64> {
        self.get_time_index()
    }

    fn get_time_index(&mut self) -> ModelResult<u64> {
        if let Some(idx) = self.cached_time_idx {
            return Ok(idx);
        }

        let idx = self.client.get_time_idx().map_err(|e| e.to_string())?;
        self.cached_time_idx = Some(idx);
        Ok(idx)
    }
}
