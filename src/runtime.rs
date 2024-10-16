use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
};

use crate::convert::Mappable;
use crate::waveloader::{self, WellenSignalExt};
use gdbstub::common::Pid;
use wellen::{TimeTable, TimeTableIdx};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Event {
    DoneStep,
    Halted,
    Break,
    WatchWrite(u32),
    WatchRead(u32),
}

pub struct WaveCursor {
    pub time_idx: TimeTableIdx,
    pub all_changes: Vec<TimeTableIdx>,
    pub all_times: TimeTable,
}

pub enum ExecMode {
    Step,
    Continue,
    RangeStep(u32, u32),
}

pub struct Waver {
    pub waves: RequiredWaves,
    pub cursor: WaveCursor,
    pub map: waveloader::Mapping,
    pub breakpoints: Vec<u32>,
    pub exec_mode: ExecMode,
}

impl Waver {
    pub fn new(wave_path: PathBuf, signal_mapping_path: PathBuf) -> anyhow::Result<Self> {
        let mut mapping = String::new();
        let _ = std::fs::File::open(signal_mapping_path)?.read_to_string(&mut mapping)?;
        let mapping = serde_yaml::from_str(mapping.as_str())?;
        let loaded = waveloader::Loaded::create_loaded_waves(
            wave_path.to_string_lossy().to_string(),
            &mapping,
        )?;
    }
    pub fn get_current_pc<T: Mappable>(&self) -> T {
        T::from_signal(self.waves.pc.get_val(self.cursor.time_idx))
    }
}

pub struct RequiredWaves {
    pub pc: wellen::Signal,
    pub grps: Vec<wellen::Signal>,
    //fprs: Option<[wellen::Signal; 32]>,
    //csrs: HashMap<u32, wellen::Signal>,
}
