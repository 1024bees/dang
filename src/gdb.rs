use std::{io::Write};

use crate::runtime::{ExecMode, Waver};
use crate::waveloader;
use crate::{convert::Mappable};
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
use gdbstub::{target::ext::base::singlethread::SingleThreadResume};
use gdbstub::{
    common::Signal,
    target::{self, Target, TargetResult},
};
use gdbstub::{
    outputln,
    target::ext::{
        base::singlethread::SingleThreadBase,
        monitor_cmd::MonitorCmd,
    },
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
        let cmd = match core::str::from_utf8(cmd) {
            Ok(cmd) => cmd,
            Err(_) => {
                outputln!(out, "command must be valid UTF-8");
                return Ok(());
            }
        };

        match cmd {
            "" => outputln!(out,
                "WHAT DID YOU SAY?! SPEAK UP! I WILL CRAWL THROUGH THE TERMINAL :)! I AM JUST BEING SILLY!"
            ),
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
        //TODO: support this
        //
        //Some(self)
        None
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

impl SingleThreadBase for Waver {
    fn read_registers(
        &mut self,
        regs: &mut <Riscv32 as Arch>::Registers,
    ) -> TargetResult<(), Self> {
        let idx = self.cursor.time_idx;
        regs.pc = u32::from_signal(self.waves.pc.get_val(idx));

        for i in 0..32 {
            regs.x[i] = u32::from_signal(self.waves.grps[i].get_val(idx));
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

    fn read_addrs(&mut self, _start_addr: u32, _data: &mut [u8]) -> TargetResult<usize, Self> {
        //TODO: add support for reading memory eventually, eventually
        Ok(0)
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
        match reg_id {
            RiscvRegId::Gpr(grp_id) => {
                let val =
                    u32::from_signal(self.waves.grps[grp_id as usize].get_val(idx)).to_be_bytes();
                // Use the write method directly on buf
                match buf.write(&val) {
                    Ok(bytes_written) => Ok(bytes_written), // Return the number of bytes written
                    Err(_) => Ok(0),
                }
            }
            _ => Ok(0),
        }
    }

    fn write_register(
        &mut self,
        _tid: (),
        _reg_id: <Riscv32 as Arch>::RegId,
        _val: &[u8],
    ) -> TargetResult<(), Self> {
        Err(().into())
    }
}

impl target::ext::base::reverse_exec::ReverseCont<()> for Waver {
    fn reverse_cont(&mut self) -> Result<(), Self::Error> {
        // FIXME: actually implement reverse step
        eprintln!(
            "FIXME: Not actually reverse-continuing. Performing forwards continue instead..."
        );
        self.exec_mode = ExecMode::Continue;
        Ok(())
    }
}

impl target::ext::base::reverse_exec::ReverseStep<()> for Waver {
    fn reverse_step(&mut self, _tid: ()) -> Result<(), Self::Error> {
        // FIXME: actually implement reverse step

        eprintln!(
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
