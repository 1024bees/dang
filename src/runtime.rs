use std::{io::Read, path::PathBuf};

use crate::waveloader::{self, WellenSignalExt};
use crate::{convert::Mappable, waveloader::Loaded};

use wellen::{TimeTable, TimeTableIdx};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Event {
    DoneStep,
    Halted,
    Break,
    //TODO -- add this in
    //WatchWrite(u32),
    //WatchRead(u32),
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

    pub breakpoints: Vec<u32>,
    pub exec_mode: ExecMode,
}

impl Waver {
    pub fn new(wave_path: PathBuf, py_file_path: PathBuf) -> anyhow::Result<Self> {
        let Loaded { cursor, waves } =
            waveloader::Loaded::create_loaded_waves(wave_path, py_file_path)?;
        Ok(Waver {
            waves,
            cursor,

            breakpoints: Vec::new(),
            exec_mode: ExecMode::Step,
        })
    }
    pub fn get_current_pc<T: Mappable>(&self) -> T {
        T::from_signal(self.waves.pc.get_val(self.cursor.time_idx))
    }

    /// single-step the interpreter
    pub fn step(&mut self) -> Option<Event> {
        let maybe_next = self.waves.pc.try_get_next_val(self.cursor.time_idx);
        if let Some((maybe_pc_sig, _idx)) = maybe_next {
            let maybe_pc = u32::try_from_signal(maybe_pc_sig);
            if let Some(pc) = maybe_pc {
                if self.breakpoints.contains(&pc) {
                    return Some(Event::Break);
                }
                None
            } else {
                let sig_str = maybe_pc_sig.to_bit_string().unwrap();
                eprintln!("PC could not be extracted as a u32 from the PC signal -- extracted value is {sig_str}");
                Some(Event::Halted)
            }
        } else {
            Some(Event::Halted)
        }
    }

    /// run the emulator in accordance with the currently set `ExecutionMode`.
    ///
    /// since the emulator runs in the same thread as the GDB loop, the emulator
    /// will use the provided callback to poll the connection for incoming data
    /// every 1024 steps.
    pub fn run(&mut self, mut poll_incoming_data: impl FnMut() -> bool) -> RunEvent {
        match self.exec_mode {
            ExecMode::Step => RunEvent::Event(self.step().unwrap_or(Event::DoneStep)),
            ExecMode::Continue => {
                let mut cycles = 0;
                loop {
                    if cycles % 1024 == 0 {
                        // poll for incoming data
                        if poll_incoming_data() {
                            break RunEvent::IncomingData;
                        }
                    }
                    cycles += 1;

                    if let Some(event) = self.step() {
                        break RunEvent::Event(event);
                    };
                }
            }
            // just continue, but with an extra PC check
            ExecMode::RangeStep(start, end) => {
                let mut cycles = 0;
                loop {
                    if cycles % 1024 == 0 {
                        // poll for incoming data
                        if poll_incoming_data() {
                            break RunEvent::IncomingData;
                        }
                    }
                    cycles += 1;

                    if let Some(event) = self.step() {
                        break RunEvent::Event(event);
                    };

                    if !(start..end).contains(&self.get_current_pc()) {
                        break RunEvent::Event(Event::DoneStep);
                    }
                }
            }
        }
    }
}

pub enum RunEvent {
    IncomingData,
    Event(Event),
}

pub struct RequiredWaves {
    pub pc: wellen::Signal,
    pub gprs: Vec<wellen::Signal>,
    //fprs: Option<[wellen::Signal; 32]>,
    //csrs: HashMap<u32, wellen::Signal>,
}
