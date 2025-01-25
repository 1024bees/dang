use std::collections::BTreeMap;
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

#[derive(Debug)]
pub enum ExecMode {
    Step,
    Continue,
    RangeStep(u32, u32),
}

pub struct Waver {
    pub waves: RequiredWaves,
    pub cursor: WaveCursor,
    pub mem: DummyMem,
    pub breakpoints: Vec<u32>,
    pub exec_mode: ExecMode,
}

#[derive(Default)]
pub struct DummyMem {
    mem: BTreeMap<u32, u8>,
}

impl DummyMem {
    pub fn w8(&mut self, addr: u32, val: u8) {
        self.mem.insert(addr, val);
    }

    pub fn r8(&self, addr: u32) -> u8 {
        self.mem.get(&addr).copied().unwrap_or(0)
    }
}

impl Waver {
    pub fn reset(&mut self) {
        self.cursor.time_idx = 0;
    }

    pub fn new(
        wave_path: PathBuf,
        py_file_path: PathBuf,
        elf_path: PathBuf,
    ) -> anyhow::Result<Self> {
        let Loaded { cursor, waves } =
            waveloader::Loaded::create_loaded_waves(wave_path, py_file_path)?;
        // load ELF
        let program_elf = std::fs::read(elf_path)?;
        let elf_header = goblin::elf::Elf::parse(&program_elf)?;

        let mut mem = DummyMem::default();

        // copy all in-memory sections from the ELF file into system RAM
        let sections = elf_header
            .section_headers
            .iter()
            .filter(|h| h.is_alloc() && h.sh_type != goblin::elf::section_header::SHT_NOBITS);

        for h in sections {
            eprintln!(
                "loading section {:?} into memory from [{:#010x?}..{:#010x?}]",
                elf_header.shdr_strtab.get_at(h.sh_name).unwrap(),
                h.sh_addr,
                h.sh_addr + h.sh_size,
            );

            for (i, b) in program_elf[h.file_range().unwrap()].iter().enumerate() {
                mem.w8(h.sh_addr as u32 + i as u32, *b);
            }
        }

        Ok(Waver {
            waves,
            cursor,
            mem,
            breakpoints: Vec::new(),
            exec_mode: ExecMode::Step,
        })
    }
    pub fn get_current_pc<T: Mappable>(&self) -> T {
        T::from_signal(self.waves.pc.get_val(self.cursor.time_idx))
    }

    pub fn get_current_gpr(&self, idx: usize) -> u32 {
        u32::from_signal(self.waves.gprs[idx].get_val(self.cursor.time_idx))
    }

    pub fn next_pc(&mut self) -> Option<u32> {
        let prev_pc: u32 = self.get_current_pc();
        let (new_pc, idx) = self
            .waves
            .pc
            .try_get_next_val(self.cursor.time_idx)
            .map(|(sig, _idx)| (u32::try_from_signal(sig), _idx))?;
        self.cursor.time_idx = idx;
        if Some(prev_pc) == new_pc {
            None
        } else {
            new_pc
        }
    }

    /// single-step the interpreter
    pub fn step(&mut self) -> Option<Event> {
        let next_pc = self.next_pc();
        if let Some(pc) = next_pc {
            log::info!("pc is {:?}", pc);
            if self.breakpoints.contains(&pc) {
                return Some(Event::Break);
            }
            None
        } else {
            let current_pc: u32 = self.get_current_pc();
            log::info!("Could not advance past current pc-- extracted value is {current_pc}");
            Some(Event::Halted)
        }
    }

    /// run the emulator in accordance with the currently set `ExecutionMode`.
    ///
    /// since the emulator runs in the same thread as the GDB loop, the emulator
    /// will use the provided callback to poll the connection for incoming data
    /// every 1024 steps.
    pub fn run(&mut self, mut poll_incoming_data: impl FnMut() -> bool) -> RunEvent {
        let run_event = match self.exec_mode {
            ExecMode::Step => RunEvent::Event(self.step().unwrap_or(Event::DoneStep)),
            ExecMode::Continue => {
                let mut cycles = 0;
                loop {
                    if cycles % 1024 == 0 {
                        log::info!("executed {} cycles", cycles);
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
        };
        log::info!("run_event is {:?}", run_event);
        run_event
    }
}

#[derive(Debug)]
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
