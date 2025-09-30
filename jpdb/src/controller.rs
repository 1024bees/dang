use shucks::Var;

use crate::model::{DebuggerModel, ExecutionSnapshot, ModelResult, SignalSnapshot, SourceSnapshot};
use crate::view::ViewState;

pub struct Controller {
    model: DebuggerModel,
}

impl Controller {
    pub fn new(model: DebuggerModel) -> Self {
        Self { model }
    }

    pub fn refresh_all_views(&mut self, view: &mut ViewState) {
        if let Ok(execution) = self.fetch_execution_snapshot() {
            Self::apply_execution_snapshot(view, execution);
        } else {
            view.execution_lines = vec!["Failed to load execution info".to_string()];
            view.instruction_lines = vec!["Failed to load execution info".to_string()];
        }

        if let Ok(source) = self.fetch_source_snapshot() {
            view.source_lines = source.lines;
        } else {
            view.source_lines = vec!["Failed to load source info".to_string()];
        }

        if let Ok(signals) = self.fetch_signal_snapshot() {
            view.signal_lines = signals.lines;
        } else {
            view.signal_lines = vec!["Failed to load signal info".to_string()];
        }
    }

    pub fn refresh_signal_view(&mut self, view: &mut ViewState) {
        match self.fetch_signal_snapshot() {
            Ok(snapshot) => view.signal_lines = snapshot.lines,
            Err(err) => {
                view.signal_lines = vec![format!("Error getting signal info: {err}")];
            }
        }
    }

    pub fn step(&mut self) -> ModelResult<()> {
        self.model.step()
    }

    pub fn continue_execution(&mut self) -> ModelResult<()> {
        self.model.continue_execution()
    }

    pub fn set_breakpoint(&mut self, address: u32) -> ModelResult<()> {
        self.model.set_breakpoint(address)
    }

    pub fn set_breakpoint_at_line(&mut self, file: &str, line: u64) -> ModelResult<Vec<u32>> {
        self.model.set_breakpoint_at_line(file, line)
    }

    pub fn fuzzy_match_signals(&mut self, query: &str) -> Vec<(Var, String)> {
        self.model.fuzzy_match_signals(query)
    }

    pub fn select_signal(&mut self, var: Var) {
        self.model.select_signal(var);
    }

    pub fn invalidate_time_index(&mut self) {
        self.model.invalidate_time_index();
    }

    fn fetch_execution_snapshot(&mut self) -> ModelResult<ExecutionSnapshot> {
        self.model.fetch_execution_snapshot()
    }

    fn fetch_source_snapshot(&mut self) -> ModelResult<SourceSnapshot> {
        self.model.fetch_source_snapshot()
    }

    fn fetch_signal_snapshot(&mut self) -> ModelResult<SignalSnapshot> {
        self.model.fetch_signal_snapshot()
    }

    fn apply_execution_snapshot(view: &mut ViewState, snapshot: ExecutionSnapshot) {
        view.execution_lines = snapshot.summary_lines;
        view.instruction_lines = snapshot.instruction_lines;
    }
}
