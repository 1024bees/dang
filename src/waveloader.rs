use crate::{
    convert::Mappable,
    runtime::{RequiredWaves, WaveCursor},
};
use std::collections::HashMap;

use anyhow::{anyhow, Result};
use wellen::{
    self, GetItem, Hierarchy, LoadOptions, Signal, SignalRef, SignalValue, TimeTableIdx, VarRef,
};

use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
pub struct Loaded {
    waves: RequiredWaves,
}
const LOAD_OPTS: LoadOptions = LoadOptions {
    multi_thread: true,
    remove_scopes_with_empty_name: false,
};

use serde::Deserialize;
#[derive(Default, Deserialize)]
pub struct Mapping {
    pc: String,
    gprs: [String; 32],
}

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
        if let Some(offset) = data_offset {
            Some(self.get_value_at(&offset, 0))
        } else {
            None
        }
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
    pub fn create_loaded_waves(file_name: String, signal_map: &Mapping) -> Result<Self> {
        let header = wellen::viewers::read_header(file_name.as_str(), &LOAD_OPTS)?;
        let hierarchy = header.hierarchy;

        let mut body = wellen::viewers::read_body(header.body, &hierarchy, None)?;

        let pc_var = path_to_signal_ref(&hierarchy, signal_map.pc.as_str())?;
        let gprs = signal_map
            .gprs
            .iter()
            .map(|val| path_to_signal_ref(&hierarchy, val))
            .collect::<anyhow::Result<Vec<SignalRef>>>()?;
        let pc = body
            .source
            .load_signals(&[pc_var], &hierarchy, true)
            .remove(0)
            .1;
        let grps: Vec<Signal> = body
            .source
            .load_signals(gprs.as_slice(), &hierarchy, true)
            .into_iter()
            .map(|val| val.1)
            .collect();
        let mut all_changes_together = vec![];
        all_changes_together.push(pc.time_indices());
        for gpr in grps.iter() {
            all_changes_together.push(gpr.time_indices());
        }
        let all_changes = merge_changes(all_changes_together);

        Ok(Loaded {
            waves: RequiredWaves { pc, grps },
        })
    }
}
