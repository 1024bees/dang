use crate::convert::Mappable;
use crate::runtime::{RequiredWaves, WaveCursor};

use anyhow::Result;
use pyo3::prelude::*;
use pywellen::{self, pywellen as doggy};
use wellen::{self, LoadOptions, Signal, SignalValue, TimeTableIdx};

use std::{cmp::Ordering, collections::HashMap, fs, path::Path};
use std::{cmp::Reverse, sync::Once};
use std::{collections::BinaryHeap, path::PathBuf};
pub struct Loaded {
    pub(crate) waves: RequiredWaves,
    pub(crate) cursor: WaveCursor,
}
const LOAD_OPTS: LoadOptions = LoadOptions {
    multi_thread: true,
    remove_scopes_with_empty_name: false,
};

pub trait WellenSignalExt {
    /// Trivially maps idx to the first value available
    fn try_get_val(&self, idx: TimeTableIdx) -> Option<SignalValue<'_>>;
    fn try_get_next_val(&self, idx: TimeTableIdx) -> Option<(SignalValue<'_>, TimeTableIdx)>;

    fn find_idx<T: Mappable>(&self, value: T) -> Option<TimeTableIdx>;

    fn get_val(&self, idx: TimeTableIdx) -> SignalValue<'_> {
        self.try_get_val(idx).unwrap()
    }
}

impl WellenSignalExt for Signal {
    fn try_get_next_val(&self, idx: TimeTableIdx) -> Option<(SignalValue<'_>, TimeTableIdx)> {
        let data_offset_and_idx = self.get_offset(idx).and_then(|val| {
            val.next_index
                .and_then(|ni| self.get_offset(ni.into()).map(|offset| (offset, ni)))
        });
        if let Some((offset, idx)) = data_offset_and_idx {
            Some((self.get_value_at(&offset, 0), idx.into()))
        } else {
            None
        }
    }

    /// Finds the index of the first value in the signal that matches the given value
    ///
    /// This is a linear search, so it is not efficient for large signals.
    fn find_idx<T: Mappable>(&self, value: T) -> Option<TimeTableIdx> {
        self.time_indices()
            .iter()
            .position(|idx| {
                T::try_from_signal(self.get_val(*idx))
                    .map(|val| val == value)
                    .unwrap_or(false)
            })
            .map(|idx| idx as u32)
    }

    fn try_get_val(&self, idx: TimeTableIdx) -> Option<SignalValue<'_>> {
        let data_offset = self.get_offset(idx);
        let val = data_offset.map(|offset| self.get_value_at(&offset, 0));

        val
    }
}

#[derive(Debug, Eq)]
struct Item<'a> {
    arr: &'a [TimeTableIdx],
    idx: usize,
}

impl PartialEq for Item<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.get_item() == other.get_item()
    }
}

impl PartialOrd for Item<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.get_item().cmp(&other.get_item()))
    }
}

impl Ord for Item<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_item().cmp(&other.get_item())
    }
}

impl<'a> Item<'a> {
    fn new(arr: &'a [TimeTableIdx], idx: usize) -> Self {
        Self { arr, idx }
    }

    fn get_item(&self) -> TimeTableIdx {
        self.arr[self.idx]
    }
}

fn merge_changes(arrays: Vec<&[TimeTableIdx]>) -> Vec<TimeTableIdx> {
    let mut sorted = vec![];

    let mut heap = BinaryHeap::with_capacity(arrays.len());
    for arr in arrays {
        let item = Item::new(arr, 0);
        heap.push(Reverse(item));
    }

    while !heap.is_empty() {
        let mut it = heap.pop().unwrap();
        sorted.push(it.0.get_item());
        it.0.idx += 1;
        if it.0.idx < it.0.arr.len() {
            heap.push(it)
        }
    }

    sorted
}

impl Loaded {
    pub fn create_loaded_waves(
        file_name: PathBuf,
        signal_py_file: PathBuf,
        first_pc: u32,
    ) -> Result<Self> {
        let header = wellen::viewers::read_header(file_name.as_path(), &LOAD_OPTS)?;
        let hierarchy = header.hierarchy;

        let body = wellen::viewers::read_body(header.body, &hierarchy, None)?;

        let script_name = "get_gdb_signals";
        let script_output =
            validate_get_signals(signal_py_file.as_path(), script_name, file_name.as_path());
        if script_output.signals.is_none() {
            return Err(anyhow::anyhow!("Failed to validate get_gdb_signals"));
        }
        let mut py_signals = script_output.signals.unwrap();

        let pc = py_signals
            .remove("pc")
            .expect("No signal provided named pc!");

        let gprs: Vec<Signal> = (0..32)
            .map(|val| {
                py_signals
                    .remove(format!("x{val}").as_str())
                    .expect("No signal named x{val} provided")
            })
            .collect();

        let mut all_changes_together = vec![];
        all_changes_together.push(pc.time_indices());
        for gpr in gprs.iter() {
            all_changes_together.push(gpr.time_indices());
        }
        let all_changes = merge_changes(all_changes_together);
        let first_pc_idx = pc.find_idx(first_pc).unwrap();
        log::debug!("found first PC index: {first_pc_idx}");
        let cursor = WaveCursor {
            time_idx: first_pc_idx,
            all_changes,
            all_times: body.time_table,
        };

        Ok(Loaded {
            waves: RequiredWaves { pc, gprs },
            cursor,
        })
    }
}

static INIT: Once = std::sync::Once::new();

fn initialize() {
    INIT.call_once(|| {
        pyo3::append_to_inittab!(doggy);
    });
}

pub enum MappingParsedEvents {
    FileStatus,
    FunctionStatus,
    /// If error message is null, it succeeded
    WaveCreationStatus,
    /// If error essage is null, it succeeded
    GetFnCall,
}

pub struct ValidationResult {
    /// If None, it failed
    pub signals: Option<HashMap<String, wellen::Signal>>,
}

impl ValidationResult {
    pub fn from_events(_all_events: Vec<MappingParsedEvents>) -> Self {
        Self {
            signals: None,
        }
    }
}

pub fn validate_get_signals(script: &Path, fn_name: &str, wave_path: &Path) -> ValidationResult {
    initialize();
    let mut events = vec![];

    let script_content = fs::read_to_string(script);
    if let Err(_) = script_content {
        events.push(MappingParsedEvents::FileStatus);
        return ValidationResult::from_events(events);
    }
    events.push(MappingParsedEvents::FileStatus);
    let script_content = script_content.unwrap();

    pyo3::prepare_freethreaded_python();
    let py_result = Python::with_gil(|py| {
        let activators =
            PyModule::from_code_bound(py, script_content.as_str(), "signal_get.py", "signal_get");

        let activators = match activators {
            Ok(module) => module,
            Err(e) => {
                events.push(MappingParsedEvents::WaveCreationStatus);
                return Err(e);
            }
        };

        let wave_result =
            pywellen::Waveform::new(wave_path.to_string_lossy().to_string(), true, true);
        let wave = match wave_result {
            Ok(w) => {
                events.push(MappingParsedEvents::WaveCreationStatus);
                w
            }
            Err(e) => {
                events.push(MappingParsedEvents::WaveCreationStatus);
                return Err(pyo3::PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    e.to_string(),
                ));
            }
        };

        let wave_bound = Bound::new(py, wave)?;

        let function_result = activators.getattr(fn_name);
        let function = match function_result {
            Ok(f) => {
                events.push(MappingParsedEvents::FunctionStatus);
                f
            }
            Err(e) => {
                events.push(MappingParsedEvents::FunctionStatus);
                events.push(MappingParsedEvents::GetFnCall);
                return Err(e);
            }
        };

        let call_result = function.call1((wave_bound,));
        let all_waves: HashMap<String, pywellen::Signal> = match call_result {
            Ok(result) => {
                events.push(MappingParsedEvents::GetFnCall);
                result.extract()?
            }
            Err(e) => {
                events.push(MappingParsedEvents::GetFnCall);
                return Err(e);
            }
        };

        Ok(all_waves)
    });

    // Check for required GDB signals regardless of Python execution result
    let mut missing_signals = vec![];

    let signals = if let Ok(py_signals) = py_result {
        // Convert to wellen signals
        let wellen_signals: HashMap<String, wellen::Signal> = py_signals
            .into_iter()
            .filter_map(|(name, signal)| signal.to_wellen_signal().map(|s| (name, s)))
            .collect();

        // Check for required signals
        if !wellen_signals.contains_key("pc") {
            missing_signals.push("pc".to_string());
        }

        for i in 0..32 {
            let signal_name = format!("x{}", i);
            if !wellen_signals.contains_key(&signal_name) {
                missing_signals.push(signal_name);
            }
        }
        Some(wellen_signals)
    } else {
        None
    };

    ValidationResult {
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn test_execute_get_signals() {
        // Get the path to the test script
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let script_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/signal_get.py");

        // Read the script content

        // Define the function name and wave path
        let fn_name = "get_gdb_signals";
        let wave_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/sim.fst");

        // Call the function
        let result = validate_get_signals(script_path.as_path(), fn_name, wave_path.as_path());

        // Check the result
        match result.signals {
            Some(signals) => {
                dbg!(&signals);
                // Perform assertions on the signals
                assert!(!signals.is_empty(), "Signals should not be empty");
                // Add more assertions as needed
                //
            }
            None => panic!("Function execution failed"),
        }
    }
}
