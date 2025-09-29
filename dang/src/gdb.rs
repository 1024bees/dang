use std::io::Write;

use crate::convert::Mappable;
use crate::runtime::{ExecMode, Waver};
use crate::waveloader;
use gdbstub::common::Pid;
use gdbstub::target::ext::base::singlethread::SingleThreadResume;
use gdbstub::target::ext::extended_mode::{Args, AttachKind, ShouldTerminate};
use gdbstub::{
    arch::Arch,
    target::{
        ext::{
            breakpoints::Breakpoints,
            monitor_cmd::ConsoleOutput,
            section_offsets::{Offsets, SectionOffsets},
        },
        TargetError,
    },
};
use gdbstub::{
    common::Signal,
    target::{self, Target, TargetResult},
};
use gdbstub::{
    outputln,
    target::ext::{base::singlethread::SingleThreadBase, monitor_cmd::MonitorCmd},
};
use gdbstub_arch::riscv::{reg::id::RiscvRegId, Riscv32};
use waveloader::WellenSignalExt;

impl Breakpoints for Waver {
    #[inline(always)]
    fn support_sw_breakpoint(
        &mut self,
    ) -> Option<target::ext::breakpoints::SwBreakpointOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_hw_watchpoint(
        &mut self,
    ) -> Option<target::ext::breakpoints::HwWatchpointOps<'_, Self>> {
        None
    }
}

impl target::ext::breakpoints::SwBreakpoint for Waver {
    fn add_sw_breakpoint(
        &mut self,
        addr: u32,
        _kind: <Riscv32 as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        self.breakpoints.push(addr);
        Ok(true)
    }

    fn remove_sw_breakpoint(
        &mut self,
        addr: u32,
        _kind: <Riscv32 as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        match self.breakpoints.iter().position(|x| *x == addr) {
            None => return Ok(false),
            Some(pos) => self.breakpoints.remove(pos),
        };

        Ok(true)
    }
}

impl MonitorCmd for Waver {
    fn handle_monitor_cmd(
        &mut self,
        cmd: &[u8],
        mut out: ConsoleOutput<'_>,
    ) -> Result<(), Self::Error> {
        log::info!("DANG SERVER: Received monitor command (QRcmd) with raw bytes: {cmd:?}");
        let cmd = match core::str::from_utf8(cmd) {
            Ok(cmd) => cmd,
            Err(_) => {
                outputln!(out, "command must be valid UTF-8");
                return Ok(());
            }
        };
        log::info!("DANG SERVER: Processing monitor command: '{cmd}'");

        match cmd {
            "" => outputln!(out,
                "WHAT DID YOU SAY?! SPEAK UP! I WILL CRAWL THROUGH THE TERMINAL :)! I AM JUST BEING SILLY!"
            ),
            "time_idx" => {
                let time_idx = self.cursor.time_idx;
                log::info!("DANG SERVER: time_idx command returning: {time_idx}");
                outputln!(out, "{}", time_idx)
            },
            _ => outputln!(out, "I don't know how to handle '{}'", cmd),
        };

        Ok(())
    }
}

impl SectionOffsets for Waver {
    fn get_section_offsets(&mut self) -> Result<Offsets<u32>, Self::Error> {
        Ok(Offsets::Sections {
            text: 0,
            data: 0,
            bss: None,
        })
    }
}

impl Target for Waver {
    type Error = &'static str;
    type Arch = Riscv32;

    // --------------- IMPORTANT NOTE ---------------
    // Always remember to annotate IDET enable methods with `inline(always)`!
    // Without this annotation, LLVM might fail to dead-code-eliminate nested IDET
    // implementations, resulting in unnecessary binary bloat.

    #[inline(always)]
    fn base_ops(&mut self) -> target::ext::base::BaseOps<'_, Self::Arch, Self::Error> {
        target::ext::base::BaseOps::SingleThread(self)
    }

    #[inline(always)]
    fn support_breakpoints(
        &mut self,
    ) -> Option<target::ext::breakpoints::BreakpointsOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_extended_mode(
        &mut self,
    ) -> Option<target::ext::extended_mode::ExtendedModeOps<'_, Self>> {
        None
    }

    #[inline(always)]
    fn support_monitor_cmd(&mut self) -> Option<target::ext::monitor_cmd::MonitorCmdOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_section_offsets(
        &mut self,
    ) -> Option<target::ext::section_offsets::SectionOffsetsOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_target_description_xml_override(
        &mut self,
    ) -> Option<
        target::ext::target_description_xml_override::TargetDescriptionXmlOverrideOps<'_, Self>,
    > {
        None
    }

    #[inline(always)]
    fn support_lldb_register_info_override(
        &mut self,
    ) -> Option<target::ext::lldb_register_info_override::LldbRegisterInfoOverrideOps<'_, Self>>
    {
        None
    }

    #[inline(always)]
    fn support_memory_map(&mut self) -> Option<target::ext::memory_map::MemoryMapOps<'_, Self>> {
        None
    }

    #[inline(always)]
    fn support_catch_syscalls(
        &mut self,
    ) -> Option<target::ext::catch_syscalls::CatchSyscallsOps<'_, Self>> {
        None
    }

    #[inline(always)]
    fn support_host_io(&mut self) -> Option<target::ext::host_io::HostIoOps<'_, Self>> {
        None
    }

    #[inline(always)]
    fn support_exec_file(&mut self) -> Option<target::ext::exec_file::ExecFileOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_auxv(&mut self) -> Option<target::ext::auxv::AuxvOps<'_, Self>> {
        None
    }

    #[inline(always)]
    fn support_libraries_svr4(
        &mut self,
    ) -> Option<target::ext::libraries::LibrariesSvr4Ops<'_, Self>> {
        None
    }
}

fn copy_range_to_buf(src: &[u8], offset: u64, length: usize, dest: &mut [u8]) -> Result<usize, ()> {
    let start = offset as usize;
    if start >= src.len() {
        // offset is beyond the end of src, no data copied
        return Ok(0);
    }

    // Determine how many bytes we can actually copy
    let end = (start + length).min(src.len());
    let copy_len = end - start;
    let copy_len = copy_len.min(dest.len());

    // Copy the bytes into dest
    dest[..copy_len].copy_from_slice(&src[start..start + copy_len]);

    // Return how many bytes were actually copied
    Ok(copy_len)
}

impl target::ext::exec_file::ExecFile for Waver {
    fn get_exec_file(
        &self,
        _pid: Option<Pid>,
        offset: u64,
        length: usize,
        buf: &mut [u8],
    ) -> TargetResult<usize, Self> {
        // According to GDB remote protocol, qXfer:exec-file:read should return the filename path, not file contents
        let path_str = self.elf_path.to_string_lossy();
        let path_bytes = path_str.as_bytes();

        copy_range_to_buf(path_bytes, offset, length, buf).map_err(|_| TargetError::NonFatal)
    }
}

impl SingleThreadBase for Waver {
    fn read_registers(
        &mut self,
        regs: &mut <Riscv32 as Arch>::Registers,
    ) -> TargetResult<(), Self> {
        log::info!("DANG SERVER: Received read_registers command (LowerG)");
        regs.pc = self.get_current_pc();
        log::info!("reading pc; pc is {:x}", regs.pc);
        for i in 0..32 {
            log::trace!("regs {} is {:x}", i, self.get_current_gpr(i));
            regs.x[i] = self.get_current_gpr(i);
        }
        Ok(())
    }

    fn write_registers(&mut self, _regs: &<Riscv32 as Arch>::Registers) -> TargetResult<(), Self> {
        // We do not support writing registers because we have read only signals
        // We are pulling this from a waveform :)
        Err(TargetError::NonFatal)
    }

    #[inline(always)]
    fn support_single_register_access(
        &mut self,
    ) -> Option<target::ext::base::single_register_access::SingleRegisterAccessOps<'_, (), Self>>
    {
        Some(self)
    }

    fn read_addrs(&mut self, start_addr: u32, data: &mut [u8]) -> TargetResult<usize, Self> {
        log::info!("DANG SERVER: Received read_addrs command (LowerM) - reading memory from {:x} to {:x}, {} bytes",
            start_addr,
            start_addr + data.len() as u32,
            data.len()
        );
        // this is a simple emulator, with RAM covering the entire 32 bit address space
        for (addr, val) in (start_addr..).zip(data.iter_mut()) {
            *val = self.mem.r8(addr)
        }
        Ok(data.len())
    }

    fn write_addrs(&mut self, _start_addr: u32, _data: &[u8]) -> TargetResult<(), Self> {
        // We do not support writing registers because we have read only signals
        Err(TargetError::NonFatal)
    }

    #[inline(always)]
    fn support_resume(
        &mut self,
    ) -> Option<target::ext::base::singlethread::SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadResume for Waver {
    fn resume(&mut self, signal: Option<Signal>) -> Result<(), Self::Error> {
        if signal.is_some() {
            return Err("no support for continuing with signal");
        }
        self.exec_mode = ExecMode::Continue;

        Ok(())
    }

    #[inline(always)]
    fn support_reverse_cont(
        &mut self,
    ) -> Option<target::ext::base::reverse_exec::ReverseContOps<'_, (), Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_reverse_step(
        &mut self,
    ) -> Option<target::ext::base::reverse_exec::ReverseStepOps<'_, (), Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_single_step(
        &mut self,
    ) -> Option<target::ext::base::singlethread::SingleThreadSingleStepOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_range_step(
        &mut self,
    ) -> Option<target::ext::base::singlethread::SingleThreadRangeSteppingOps<'_, Self>> {
        Some(self)
    }
}

impl target::ext::base::singlethread::SingleThreadSingleStep for Waver {
    fn step(&mut self, signal: Option<Signal>) -> Result<(), Self::Error> {
        if signal.is_some() {
            return Err("no support for stepping with signal");
        }
        self.exec_mode = ExecMode::Step;

        Ok(())
    }
}

impl target::ext::base::single_register_access::SingleRegisterAccess<()> for Waver {
    fn read_register(
        &mut self,
        _tid: (),
        reg_id: <Riscv32 as Arch>::RegId,
        mut buf: &mut [u8],
    ) -> TargetResult<usize, Self> {
        let idx = self.cursor.time_idx;

        let rv = match reg_id {
            RiscvRegId::Pc => {
                let val = self.waves.pc.get_val(idx);
                let rv = u32::from_signal(val).to_be_bytes();
                match buf.write(&rv) {
                    Ok(bytes_written) => Ok(bytes_written), // Return the number of bytes written
                    Err(_) => Err(TargetError::NonFatal),
                }
            }
            RiscvRegId::Gpr(grp_id) => {
                let val = self.waves.gprs[grp_id as usize].get_val(idx);
                let val = u32::from_signal(val).to_be_bytes();
                // Use the write method directly on buf
                match buf.write(&val) {
                    Ok(bytes_written) => Ok(bytes_written), // Return the number of bytes written
                    Err(_) => Err(TargetError::NonFatal),
                }
            }
            _ => Err(TargetError::NonFatal),
        };
        if let Ok(ref inner) = rv {
            log::info!("read reg {reg_id:?}, {inner:?} bytes at idx {idx:?}");
        } else {
            log::error!("failed to read reg {reg_id:?}");
        }
        rv
    }

    fn write_register(
        &mut self,
        _tid: (),
        _reg_id: <Riscv32 as Arch>::RegId,
        _val: &[u8],
    ) -> TargetResult<(), Self> {
        Err(TargetError::NonFatal)
    }
}

impl target::ext::base::reverse_exec::ReverseCont<()> for Waver {
    fn reverse_cont(&mut self) -> Result<(), Self::Error> {
        // FIXME: actually implement reverse step
        log::info!(
            "FIXME: Not actually reverse-continuing. Performing forwards continue instead..."
        );
        self.exec_mode = ExecMode::Continue;
        Ok(())
    }
}

impl target::ext::base::reverse_exec::ReverseStep<()> for Waver {
    fn reverse_step(&mut self, _tid: ()) -> Result<(), Self::Error> {
        // FIXME: actually implement reverse step

        log::info!(
            "FIXME: Not actually reverse-stepping. Performing single forwards step instead..."
        );
        self.exec_mode = ExecMode::Step;
        Ok(())
    }
}

impl target::ext::base::singlethread::SingleThreadRangeStepping for Waver {
    fn resume_range_step(&mut self, start: u32, end: u32) -> Result<(), Self::Error> {
        self.exec_mode = ExecMode::RangeStep(start, end);
        Ok(())
    }
}

impl target::ext::extended_mode::ExtendedMode for Waver {
    fn kill(&mut self, pid: Option<Pid>) -> TargetResult<ShouldTerminate, Self> {
        log::info!("GDB sent a kill request for pid {pid:?}");
        Ok(ShouldTerminate::No)
    }

    fn restart(&mut self) -> Result<(), Self::Error> {
        log::info!("GDB sent a restart request");
        Ok(())
    }

    fn attach(&mut self, pid: Pid) -> TargetResult<(), Self> {
        log::info!("GDB attached to a process with PID {pid}");
        // stub implementation: just report the same code, but running under a
        // different pid.

        Ok(())
    }

    fn run(&mut self, filename: Option<&[u8]>, args: Args<'_, '_>) -> TargetResult<Pid, Self> {
        // simplified example: assume UTF-8 filenames / args
        //
        // To be 100% pedantically correct, consider converting to an `OsStr` in the
        // least lossy way possible (e.g: using the `from_bytes` extension from
        // `std::os::unix::ffi::OsStrExt`).

        let filename = match filename {
            None => None,
            Some(raw) => Some(core::str::from_utf8(raw).map_err(drop)?),
        };
        let args = args
            .map(|raw| core::str::from_utf8(raw).map_err(drop))
            .collect::<Result<Vec<_>, _>>()?;

        log::info!("GDB tried to run a new process with filename {filename:?}, and args {args:?}");

        self.reset();

        // when running in single-threaded mode, this PID can be anything
        Ok(Pid::new(1).unwrap())
    }

    fn query_if_attached(&mut self, pid: Pid) -> TargetResult<AttachKind, Self> {
        log::info!("GDB queried if it was attached to a process with PID {pid}");
        Ok(AttachKind::Attach)
    }

    #[inline(always)]
    fn support_configure_aslr(
        &mut self,
    ) -> Option<target::ext::extended_mode::ConfigureAslrOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_configure_env(
        &mut self,
    ) -> Option<target::ext::extended_mode::ConfigureEnvOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_configure_startup_shell(
        &mut self,
    ) -> Option<target::ext::extended_mode::ConfigureStartupShellOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_configure_working_dir(
        &mut self,
    ) -> Option<target::ext::extended_mode::ConfigureWorkingDirOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_current_active_pid(
        &mut self,
    ) -> Option<target::ext::extended_mode::CurrentActivePidOps<'_, Self>> {
        Some(self)
    }
}

impl target::ext::extended_mode::ConfigureAslr for Waver {
    fn cfg_aslr(&mut self, enabled: bool) -> TargetResult<(), Self> {
        log::info!("GDB {} ASLR", if enabled { "enabled" } else { "disabled" });
        Ok(())
    }
}

impl target::ext::extended_mode::ConfigureEnv for Waver {
    fn set_env(&mut self, key: &[u8], val: Option<&[u8]>) -> TargetResult<(), Self> {
        // simplified example: assume UTF-8 key/val env vars
        let key = core::str::from_utf8(key).map_err(drop)?;
        let val = match val {
            None => None,
            Some(raw) => Some(core::str::from_utf8(raw).map_err(drop)?),
        };

        log::info!("GDB tried to set a new env var: {key:?}={val:?}");

        Ok(())
    }

    fn remove_env(&mut self, key: &[u8]) -> TargetResult<(), Self> {
        let key = core::str::from_utf8(key).map_err(drop)?;
        log::info!("GDB tried to set remove a env var: {key:?}");

        Ok(())
    }

    fn reset_env(&mut self) -> TargetResult<(), Self> {
        log::info!("GDB tried to reset env vars");

        Ok(())
    }
}

impl target::ext::extended_mode::ConfigureStartupShell for Waver {
    fn cfg_startup_with_shell(&mut self, enabled: bool) -> TargetResult<(), Self> {
        log::info!(
            "GDB {} startup with shell",
            if enabled { "enabled" } else { "disabled" }
        );
        Ok(())
    }
}

impl target::ext::extended_mode::ConfigureWorkingDir for Waver {
    fn cfg_working_dir(&mut self, dir: Option<&[u8]>) -> TargetResult<(), Self> {
        let dir = match dir {
            None => None,
            Some(raw) => Some(core::str::from_utf8(raw).map_err(drop)?),
        };

        match dir {
            None => log::info!("GDB reset the working directory"),
            Some(dir) => log::info!("GDB set the working directory to {dir:?}"),
        }

        Ok(())
    }
}

impl target::ext::extended_mode::CurrentActivePid for Waver {
    fn current_active_pid(&mut self) -> Result<Pid, Self::Error> {
        Ok(Pid::new(1).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Waver;
    use gdbstub::target::ext::exec_file::ExecFile;
    use std::path::PathBuf;

    #[test]
    fn test_exec_file_only_reads_designated_executable() {
        // This test verifies that the ExecFile interface only accesses the designated executable
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let elf_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/hello_test.elf");
        let wave_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/sim.fst");
        let script_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/signal_get.py");

        // Create Waver with specific ELF file
        let waver = Waver::new(wave_path, script_path, elf_path.clone()).unwrap();

        // Read the actual ELF file content directly for comparison
        let expected_content = elf_path;

        // Read through the ExecFile interface
        let mut actual_content = vec![0u8; expected_content.as_os_str().len()];
        let _bytes_read =
            match waver.get_exec_file(None, 0, actual_content.len(), &mut actual_content) {
                Ok(n) => n,
                Err(_) => panic!("Should successfully read designated executable"),
            };

        let br = PathBuf::from(String::from_utf8_lossy(actual_content.as_ref()).to_string());

        assert_eq!(
            br, expected_content,
            "ExecFile interface should return exact same content as the designated executable"
        );
    }

    #[test]
    fn test_host_io_disabled() {
        // Verify that host I/O is properly disabled to prevent arbitrary file access
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let elf_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/hello_test.elf");
        let wave_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/sim.fst");
        let script_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/signal_get.py");

        let mut waver = Waver::new(wave_path, script_path, elf_path).unwrap();

        // Verify host_io support returns None (disabled)
        assert!(
            waver.support_host_io().is_none(),
            "Host I/O should be disabled to prevent arbitrary file access"
        );
    }
}
