use crate::runtime::{RequiredWaves, WaveCursor};

use anyhow::{anyhow, Result};
use pyo3::prelude::*;
use pyo3::PyResult;
use pywellen::{self, pywellen as doggy};
use wellen::{
    self, GetItem, Hierarchy, LoadOptions, Signal, SignalRef, SignalValue, TimeTableIdx, VarRef,
};

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

use serde::Deserialize;

pub trait WellenExt {
    fn get_var<S: AsRef<str>>(&self, varname: S) -> Option<VarRef>;
}

pub trait WellenSignalExt {
    /// Trivially maps idx to the first value available
    fn try_get_val(&self, idx: TimeTableIdx) -> Option<SignalValue<'_>>;
    fn try_get_next_val(&self, idx: TimeTableIdx) -> Option<(SignalValue<'_>, TimeTableIdx)>;

    fn get_val(&self, idx: TimeTableIdx) -> SignalValue<'_> {
        self.try_get_val(idx).unwrap()
    }
}

impl WellenExt for Hierarchy {
    fn get_var<S: AsRef<str>>(&self, varname: S) -> Option<VarRef> {
        let varname = varname.as_ref();
        let vars: Vec<&str> = varname.split('.').collect();
        let vals = &vars[0..vars.len() - 1];
        let last = vars.last().unwrap();
        self.lookup_var(vals, last)
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

    fn try_get_val(&self, idx: TimeTableIdx) -> Option<SignalValue<'_>> {
        let data_offset = self.get_offset(idx);
        data_offset.map(|offset| self.get_value_at(&offset, 0))
    }
}

fn path_to_signal_ref(hier: &Hierarchy, path: impl AsRef<str>) -> anyhow::Result<SignalRef> {
    hier.get_var(path)
        .ok_or(anyhow!("No signal  found"))
        .map(|val| hier.get(val).signal_ref())
}

#[derive(Debug, Eq)]
struct Item<'a> {
    arr: &'a [TimeTableIdx],
    idx: usize,
}

impl<'a> PartialEq for Item<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.get_item() == other.get_item()
    }
}

impl<'a> PartialOrd for Item<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.get_item().partial_cmp(&other.get_item())
    }
}

impl<'a> Ord for Item<'a> {
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
    pub fn create_loaded_waves(file_name: PathBuf, signal_py_file: PathBuf) -> Result<Self> {
        let header = wellen::viewers::read_header(file_name.as_path(), &LOAD_OPTS)?;
        let hierarchy = header.hierarchy;

        let mut body = wellen::viewers::read_body(header.body, &hierarchy, None)?;

        let script_name = "get_signals";
        let mut py_signals =
            execute_get_signals(signal_py_file.as_path(), script_name, file_name.as_path())?;

        let pc = py_signals
            .remove("pc")
            .expect("No signal provided named pc!");

        let gprs: Vec<Signal> = (0..31)
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
        let cursor = WaveCursor {
            time_idx: 0,
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

pub fn execute_get_signals(
    script: &Path,
    fn_name: &str,
    wave_path: &Path,
) -> PyResult<HashMap<String, wellen::Signal>> {
    initialize();

    let script_content = fs::read_to_string(script).expect("Failed to read script file");

    pyo3::prepare_freethreaded_python();
    let val = {
        let val: PyResult<HashMap<String, pywellen::Signal>> = Python::with_gil(|py| {
            let activators = PyModule::from_code_bound(
                py,
                script_content.as_str(),
                "signal_get.py",
                "signal_get",
            )?;
            let wave = Bound::new(
                py,
                pywellen::Waveform::new(wave_path.to_string_lossy().to_string(), true, true)?,
            )?;

            let all_waves: HashMap<String, pywellen::Signal> =
                activators.getattr(fn_name)?.call1((wave,))?.extract()?;

            Ok(all_waves)
        });
        val
    };
    let val = val?
        .into_iter()
        .map(|(name, signal)| (name, signal.to_wellen_signal().unwrap()))
        .fold(HashMap::new(), |mut mapper, val| {
            mapper.insert(val.0, val.1);
            mapper
        });
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn test_execute_get_signals() {
        // Get the path to the test script
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let script_path = PathBuf::from(cargo_manifest_dir).join("test_data/ibex/signal_get.py");

        // Read the script content

        // Define the function name and wave path
        let fn_name = "get_signals";
        let wave_path = PathBuf::from(cargo_manifest_dir).join("test_data/ibex/sim.fst");

        // Call the function
        let result = execute_get_signals(script_path.as_path(), fn_name, wave_path.as_path());

        // Check the result
        match result {
            Ok(signals) => {
                dbg!(&signals);
                // Perform assertions on the signals
                assert!(!signals.is_empty(), "Signals should not be empty");
                // Add more assertions as needed
                //
            }
            Err(e) => panic!("Function execution failed: {:?}", e),
        }
    }
}
