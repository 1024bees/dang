//! Command line interface for jpdb

use argh::FromArgs;
use std::path::PathBuf;

#[derive(FromArgs, Debug, Clone)]
/// CLI to jpdb - JTAG Debugger
pub struct JpdbArgs {
    #[argh(option)]
    /// path to the vcd, fst or ghw file that will be stepped through
    pub wave_path: PathBuf,

    #[argh(option)]
    /// path to a signal mapping file
    pub mapping_path: PathBuf,

    #[argh(option)]
    /// path to the ELF binary
    pub elf: PathBuf,
}
