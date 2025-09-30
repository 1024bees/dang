pub struct ViewState {
    pub execution_lines: Vec<String>,
    pub instruction_lines: Vec<String>,
    pub source_lines: Vec<String>,
    pub signal_lines: Vec<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            execution_lines: Vec::new(),
            instruction_lines: Vec::new(),
            source_lines: Vec::new(),
            signal_lines: Vec::new(),
        }
    }
}
