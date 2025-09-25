use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
};

use crate::{
    commands::{Base, GdbCommand},
    response::{GdbResponse, RawGdbResponse},
    Packet,
};
use goblin::elf::Elf;
use raki::{Decode, Isa};

pub struct Client {
    strm: TcpStream,
    packet_scratch: [u8; 4096],
    elf_info: Option<ElfInfo>,
    response_buffer: Vec<u8>,
}

#[derive(Copy, Clone)]
pub enum PC {
    _64(u64),
    _32(u32),
}

impl PC {
    fn add(&self, other: u32) -> Self {
        match self {
            Self::_64(a) => Self::_64(a + other as u64),
            Self::_32(a) => Self::_32(a + other),
        }
    }
}

use raki::Instruction as RVInst;

pub struct Instruction(RVInst, PC);

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Instruction {
    pub fn pc(&self) -> &PC {
        &self.1
    }
}

impl PC {
    pub fn nz(&self) -> bool {
        match self {
            Self::_32(pc) => *pc != 0,
            Self::_64(pc) => *pc != 0,
        }
    }

    pub fn as_u32(&self) -> u32 {
        match self {
            Self::_32(pc) => *pc,
            Self::_64(pc) => *pc as u32,
        }
    }

    pub fn as_u64(&self) -> u64 {
        match self {
            Self::_32(pc) => *pc as u64,
            Self::_64(pc) => *pc,
        }
    }
}

impl std::fmt::Display for PC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::_64(pc) => {
                write!(f, "({pc})")
            }
            Self::_32(pc) => {
                write!(f, "({pc})")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ElfInfo {
    pub entry_point: u64,
    pub is_32bit: bool,
    pub machine: u16,
    pub text_section: Option<TextSectionInfo>,
    pub symbols: Vec<SymbolInfo>,
    pub elf_data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TextSectionInfo {
    pub addr: u64,
    pub size: u64,
    pub file_offset: u64,
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub addr: u64,
    pub size: u64,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self::new_with_port(9001)
    }

    pub fn new_with_port(port: u16) -> Self {
        let addr = format!("127.0.0.1:{port}");
        let strm = TcpStream::connect(addr).unwrap();
        strm.set_nodelay(true).unwrap();
        Self {
            strm,
            packet_scratch: [0; 4096],
            elf_info: None,
            response_buffer: Vec::new(),
        }
    }

    /// Drain any remaining data in the response buffer to ensure synchronization
    fn drain_response_buffer(&mut self) {
        if !self.response_buffer.is_empty() {
            log::info!(
                "Draining {} bytes from response buffer to maintain synchronization",
                self.response_buffer.len()
            );
            self.response_buffer.clear();
        }
    }

    pub fn send_command(&mut self, packet: &Packet) -> Result<RawGdbResponse, std::io::Error> {
        let pkt = packet.to_finished_packet(self.packet_scratch.as_mut_slice())?;
        log::info!("Sending packet: {packet:?}");
        self.strm.write_all(pkt.0)?;

        // Read response with proper packet handling
        let response = self.read_gdb_packet()?;
        log::info!("Read {} bytes, content is {:?}", response.len(), &response);

        let _tstr = String::from_utf8_lossy(response.as_slice());

        Ok(response)
    }

    /// Read a complete GDB packet, handling partial reads and multiple packets
    fn read_gdb_packet(&mut self) -> Result<RawGdbResponse, std::io::Error> {
        use std::io::ErrorKind;
        use std::time::{Duration, Instant};

        let timeout = Duration::from_millis(500);
        let start_time = Instant::now();

        // First, check if we have a complete packet in the buffer from previous reads
        if let Some((packet, remaining)) = Self::find_first_complete_packet(&self.response_buffer) {
            self.response_buffer = remaining;
            log::info!(
                "Returned buffered packet, {} bytes remaining in buffer",
                self.response_buffer.len()
            );
            return Ok(packet);
        }

        // Set read timeout
        self.strm
            .set_read_timeout(Some(Duration::from_millis(500)))?;

        let mut temp_buffer = [0u8; 1024];

        loop {
            match self.strm.read(&mut temp_buffer) {
                Ok(0) => {
                    // EOF - connection closed
                    break;
                }
                Ok(n) => {
                    // Add new data to our response buffer
                    self.response_buffer.extend_from_slice(&temp_buffer[..n]);
                    log::debug!(
                        "Read {} bytes, buffer now has {} bytes",
                        n,
                        self.response_buffer.len()
                    );

                    // Try to extract a complete packet from buffer - only check if we potentially have enough data
                    if self.response_buffer.len() >= 4 {
                        // Minimum packet size: $#xx
                        if let Some((packet, remaining)) =
                            Self::find_first_complete_packet(&self.response_buffer)
                        {
                            self.response_buffer = remaining;
                            log::info!(
                                "Extracted packet, {} bytes remaining in buffer",
                                self.response_buffer.len()
                            );
                            self.strm.set_read_timeout(None)?;

                            return Ok(packet);
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    // Timeout occurred, check if we have any partial data
                    if !self.response_buffer.is_empty() && start_time.elapsed() < timeout {
                        // We have partial data, keep trying for a bit longer
                        continue;
                    } else if self.response_buffer.is_empty() && start_time.elapsed() < timeout {
                        // No data yet but still within overall timeout
                        continue;
                    } else {
                        // Overall timeout exceeded
                        break;
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }

            // Overall timeout check
            if start_time.elapsed() > timeout {
                break;
            }
        }

        // Reset timeout
        self.strm.set_read_timeout(None)?;

        // If we have any data in buffer but no complete packet, return it as is
        // This handles cases where server sends malformed data
        if !self.response_buffer.is_empty() {
            if let Some((packet, remaining)) =
                Self::find_first_complete_packet(&self.response_buffer)
            {
                self.response_buffer = remaining;
                log::info!(
                    "Extracted packet, {} bytes remaining in buffer",
                    self.response_buffer.len()
                );
                self.strm.set_read_timeout(None)?;

                Ok(packet)
            } else {
                Err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    "No packet found within timeout limit",
                ))
            }
        } else {
            Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "No data received within timeout period",
            ))
        }
    }

    /// Find packet boundaries in buffer and return the first complete packet
    /// Returns (packet_data, remaining_buffer) or None if no complete packet found
    fn find_first_complete_packet(buffer: &[u8]) -> Option<(RawGdbResponse, Vec<u8>)> {
        let mdata = RawGdbResponse::find_packet_data(buffer).ok();
        if let Some(data) = mdata {
            let remaining = buffer[data.entire_packet_len()..].to_vec();
            log::debug!(
                "input buffer is {}, output is {}. remaining is {}",
                String::from_utf8_lossy(buffer),
                String::from_utf8_lossy(data.as_slice()),
                String::from_utf8_lossy(remaining.as_slice())
            );
            return Some((data, remaining));
        }
        None
    }

    pub fn send_command_parsed(
        &mut self,
        packet: Packet,
    ) -> Result<GdbResponse, Box<dyn std::error::Error>> {
        let raw_response = self.send_command(&packet)?;
        let parsed_response = GdbResponse::parse_packet(raw_response, &packet)?;
        log::info!("Parsed response: {parsed_response} from input {packet:?}");
        Ok(parsed_response)
    }

    pub fn pop_response(&mut self) -> Result<GdbResponse, Box<dyn std::error::Error>> {
        let raw_response = self.read_gdb_packet()?;
        let parsed_response = GdbResponse::parse_packet(raw_response, &Packet::default())?;
        Ok(parsed_response)
    }

    pub fn initialize_gdb_session(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Starting GDB initialization sequence...");

        // QStartNoAckMode must return OK per RSP
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QStartNoAckMode)))? {
            GdbResponse::Ack => {
                log::info!("QStartNoAckMode acknowledged with an ack");
                let resp = self.pop_response()?;
                if resp != GdbResponse::Ok {
                    return Err(format!("Expected Ok for QStartNoAckMode, got: {resp}").into());
                }
            }
            GdbResponse::Ok => {
                log::info!("QStartNoAckMode acknowledged with an ok");
            }
            other => {
                return Err(format!("Expected Ack for QStartNoAckMode, got: {other}").into());
            }
        }

        // qSupported should return a feature list; require PacketSize (commonly provided)
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QSupported)))? {
            GdbResponse::Supported { features } => {
                let has_packet_size = features.iter().any(|f| f.starts_with("PacketSize="));
                if !has_packet_size {
                    return Err(
                        format!("qSupported missing PacketSize in features: {features:?}").into(),
                    );
                }
                log::info!("qSupported features: {features:?}");
            }
            other => {
                return Err(format!("Expected qSupported feature list, got: {other}").into());
            }
        }

        // qfThreadInfo must return thread list (may be empty) or 'm...' chunk
        log::info!("About to send qfThreadInfo...");
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QfThreadInfo)))? {
            GdbResponse::ThreadInfo { threads, .. } => {
                log::info!("qfThreadInfo threads: {threads:?}");
            }
            other => {
                return Err(format!("Expected thread info for qfThreadInfo, got: {other}").into());
            }
        }

        // qsThreadInfo should continue list or return end
        log::info!("About to send qsThreadInfo...");
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QsThreadInfo)))? {
            GdbResponse::ThreadInfo { threads, .. } => {
                log::info!("qsThreadInfo threads: {threads:?}");
            }
            other => {
                return Err(format!("Expected thread info for qsThreadInfo, got: {other}").into());
            }
        }

        // '?' must return a stop reply (Sxx or Txx)
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QuestionMark)))? {
            GdbResponse::StopReply { signal, .. } => {
                log::info!("Got stop reply with signal 0x{signal:02x}");
            }
            other => {
                return Err(format!("Expected stop reply for '?', got: {other}").into());
            }
        }

        // 'g' must return register data; for RV32 expect >= 132 bytes (32 GPRs + PC), 4-byte aligned
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::LowerG)))? {
            GdbResponse::RegisterData { data } => {
                if data.len() < 132 || (data.len() % 4) != 0 {
                    return Err(format!(
                        "Unexpected register data length (got {}, expected >= 132 and multiple of 4)",
                        data.len()
                    ).into());
                }
                log::info!("Register read length OK: {} bytes", data.len());
            }
            other => {
                return Err(format!("Expected RegisterData for 'g' (LowerG), got: {other}").into());
            }
        }

        log::info!("GDB initialization sequence complete!");
        Ok(())
    }

    pub fn get_time_idx(&mut self) -> Result<u64, Box<dyn std::error::Error>> {
        let rv = self
            .send_monitor_command("time_idx")
            .inspect(|val| println!("{val}"))
            .map(|output| output.trim().parse::<u64>().map_err(|e| e.into()))?;

        rv
    }

    /// Send a monitor command to the GDB server
    pub fn send_monitor_command(
        &mut self,
        cmd: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Drain any lingering responses before sending critical commands
        self.drain_response_buffer();

        let monitor_packet = Packet::Command(GdbCommand::Base(Base::QRcmd {
            command: cmd.to_string(),
        }));

        let response = self.send_command_parsed(monitor_packet)?;

        match response {
            crate::response::GdbResponse::MonitorOutput { output } => Ok(output),
            other => Err(format!("Expected monitor output, got: {other}").into()),
        }
    }

    /// Get the executable file path from the remote target
    pub fn get_executable_path(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let response =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QXferExecFile {
                offset: 0,
                length: 1000,
            })))?;

        match response {
            crate::response::GdbResponse::QXferData { data, .. } => {
                // The data should contain the executable path as a string
                let path = String::from_utf8(data)?;
                Ok(path)
            }
            _ => Err(format!(
                "Unexpected response format for qXfer:exec-file:read, got {response:?}"
            )
            .into()),
        }
    }

    /// Parse ELF file from the given path and store information
    pub fn parse_elf_file(&mut self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let elf_data = fs::read(path)?;
        let elf = Elf::parse(&elf_data)?;

        // Check if it's 32-bit and RISC-V
        let is_32bit = elf.header.e_ident[4] == 1; // EI_CLASS: ELFCLASS32
        let is_riscv = elf.header.e_machine == 0xf3; // EM_RISCV

        if !is_riscv {
            return Err(format!(
                "Not a RISC-V binary (machine type: 0x{:x})",
                elf.header.e_machine
            )
            .into());
        }

        // Find .text section
        let text_section = elf
            .section_headers
            .iter()
            .find(|sh| {
                if let Some(name) = elf.shdr_strtab.get_at(sh.sh_name) {
                    name == ".text"
                } else {
                    false
                }
            })
            .map(|sh| TextSectionInfo {
                addr: sh.sh_addr,
                size: sh.sh_size,
                file_offset: sh.sh_offset,
            });

        // Extract symbols
        let mut symbols = Vec::new();
        for sym in &elf.syms {
            if let Some(name_str) = elf.strtab.get_at(sym.st_name) {
                if !name_str.is_empty() && sym.st_value != 0 {
                    symbols.push(SymbolInfo {
                        name: name_str.to_string(),
                        addr: sym.st_value,
                        size: sym.st_size,
                    });
                }
            }
        }

        // Sort symbols by address for efficient lookup
        symbols.sort_by_key(|s| s.addr);

        self.elf_info = Some(ElfInfo {
            entry_point: elf.header.e_entry,
            is_32bit,
            machine: elf.header.e_machine,
            text_section,
            symbols,
            elf_data,
        });

        Ok(())
    }

    /// Get 12 bytes of instruction data from ELF file starting at given PC
    pub fn get_instruction_bytes_from_elf(
        &self,
        pc: PC,
    ) -> Result<[u8; 12], Box<dyn std::error::Error>> {
        let elf_info = self
            .elf_info
            .as_ref()
            .ok_or("No ELF file loaded. Call parse_elf_file() first")?;

        let text_section = elf_info
            .text_section
            .as_ref()
            .ok_or("No .text section found in ELF file")?;

        // Check if PC is within .text section bounds
        let pc_u64 = pc.as_u64();
        if pc_u64 < text_section.addr || pc_u64 >= text_section.addr + text_section.size {
            return Err(format!(
                "PC 0x{} is outside .text section (0x{:x}-0x{:x})",
                pc,
                text_section.addr,
                text_section.addr + text_section.size
            )
            .into());
        }

        // Calculate offset in file
        let offset_in_section = pc_u64 - text_section.addr;
        let file_offset = text_section.file_offset + offset_in_section;

        // Ensure we don't read past the section boundary
        let available_bytes = (text_section.size - offset_in_section).min(12) as usize;
        if available_bytes == 0 {
            return Err("No bytes available at the specified PC".into());
        }

        // Read up to 12 bytes from the ELF data
        let mut instruction_bytes = [0u8; 12];
        let start_idx = file_offset as usize;
        let end_idx = (start_idx + available_bytes).min(elf_info.elf_data.len());

        if start_idx >= elf_info.elf_data.len() {
            return Err("File offset is beyond ELF data bounds".into());
        }

        let actual_bytes = end_idx - start_idx;
        instruction_bytes[..actual_bytes].copy_from_slice(&elf_info.elf_data[start_idx..end_idx]);

        Ok(instruction_bytes)
    }

    /// Find symbol containing the given address
    pub fn find_symbol_at_address(&self, addr: u64) -> Option<(&SymbolInfo, u64)> {
        let elf_info = self.elf_info.as_ref()?;

        // Binary search for the symbol containing this address
        match elf_info.symbols.binary_search_by(|sym| {
            if addr < sym.addr {
                std::cmp::Ordering::Greater
            } else if addr >= sym.addr + sym.size && sym.size > 0 {
                std::cmp::Ordering::Less
            } else if addr >= sym.addr && (sym.size == 0 || addr < sym.addr + sym.size) {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Less
            }
        }) {
            Ok(idx) => {
                let symbol = &elf_info.symbols[idx];
                let offset = addr - symbol.addr;
                Some((symbol, offset))
            }
            Err(idx) => {
                // Check if we're in the previous symbol (for zero-size symbols)
                if idx > 0 {
                    let symbol = &elf_info.symbols[idx - 1];
                    if addr >= symbol.addr {
                        let offset = addr - symbol.addr;
                        Some((symbol, offset))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    /// Load and parse ELF file automatically from executable path
    pub fn load_elf_info(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let elf_path = self.get_executable_path()?;
        self.parse_elf_file(&elf_path)?;
        Ok(())
    }

    /// Get the current program counter (PC) from registers
    pub fn get_current_pc(&mut self) -> Result<PC, Box<dyn std::error::Error>> {
        // Add a small delay to avoid rapid command sending that can cause response ordering issues

        let registers =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::LowerG)))?;

        match registers {
            crate::response::GdbResponse::RegisterData { data } => {
                log::debug!("Got RegisterData with {} bytes", data.len());
                if data.len() < 132 {
                    return Err(format!(
                        "Register data too short to contain PC (got {} bytes, need 132)",
                        data.len()
                    )
                    .into());
                }
                // Extract PC (assuming little-endian)
                // For RISC-V, PC is typically at the end of the register dump
                // RISC-V has 32 general purpose registers (x0-x31) of 4 bytes each = 128 bytes
                // PC is usually the next 4 bytes after that
                let pc_bytes = &data[128..132];
                let pc = u32::from_le_bytes([pc_bytes[0], pc_bytes[1], pc_bytes[2], pc_bytes[3]]);

                Ok(PC::_32(pc))
            }
            _ => {
                log::error!("Unexpected response format for register read: {registers}");
                Err(format!("Unexpected response format for register read: {registers}").into())
            }
        }
    }

    /// Show current instruction and next 3 instructions using raki decoder and ELF data
    pub fn get_current_and_next_inst(
        &mut self,
    ) -> Result<Vec<Instruction>, Box<dyn std::error::Error>> {
        // Get current PC using the dedicated method
        let pc = self.get_current_pc()?;

        log::info!("Program Counter (PC): 0x{pc}");

        // Check if we have symbol information
        if let Some((symbol, offset)) = self.find_symbol_at_address(pc.as_u64()) {
            log::info!("Current function: {} + 0x{:x}", symbol.name, offset);
        }

        // Get instruction bytes directly from ELF binary (16 bytes = 4 instructions)
        let instruction_bytes = self.get_instruction_bytes_from_elf(pc)?;

        // Determine ISA based on ELF info
        let _isa = if let Some(elf_info) = &self.elf_info {
            if elf_info.is_32bit {
                Isa::Rv32
            } else {
                Isa::Rv64
            }
        } else {
            return Err("No ELF info available. Call load_elf_info() first".into());
        };
        let mut rv = Vec::new();
        let mut start = 0;
        while start + 4 < 12 {
            let ichunk1 = &instruction_bytes[start..start + 2];

            let ichunk = &instruction_bytes[start..start + 4];

            let uu16 = u16::from_le_bytes(ichunk1.try_into().unwrap());
            let uu32 = u32::from_le_bytes(ichunk.try_into().unwrap());

            let u16inst = uu16
                .decode(Isa::Rv32)
                .inspect_err(|e| log::error!("u16 err is {e:?}, 0x{uu16:x}"))
                .inspect(|arg| log::info!("{arg}"))
                .map(|val| Instruction(val, pc.add(start as u32)))
                .ok();
            let u32inst = uu32
                .decode(Isa::Rv32)
                .map(|val| Instruction(val, pc.add(start as u32)))
                .ok();
            match (u16inst, u32inst) {
                (Some(inst16), None) => {
                    start += 2;
                    rv.push(inst16);
                }
                (None, Some(inst32)) => {
                    start += 4;
                    rv.push(inst32)
                }

                _ => {
                    if start == 0 {
                        panic!("THERE S A BOMB IN MY CAR");
                    }
                    log::info!("Done")
                }
            }
        }

        //let rv= instruction_bytes.into_iter().array_chunks::<4>().map(|val| u32::from_le_bytes(val).decode(isa)).collect();

        Ok(rv)
    }

    /// Set a software breakpoint at the specified address
    pub fn set_breakpoint(&mut self, addr: u32) -> Result<(), Box<dyn std::error::Error>> {
        let response = self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::Z0 { addr })))?;

        match response {
            crate::response::GdbResponse::Ok => Ok(()),
            _ => Err(format!("Failed to set breakpoint at address 0x{:x}: {}", addr, response).into())
        }
    }

    /// Remove a software breakpoint at the specified address
    pub fn remove_breakpoint(&mut self, addr: u32) -> Result<(), Box<dyn std::error::Error>> {
        let response = self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::Z0Remove { addr })))?;

        match response {
            crate::response::GdbResponse::Ok => Ok(()),
            _ => Err(format!("Failed to remove breakpoint at address 0x{:x}: {}", addr, response).into())
        }
    }
}

#[cfg(test)]
pub mod test_utils {
    use std::net::TcpListener;
    use std::thread;

    pub fn create_test_listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    pub fn start_dang_instance(listener: TcpListener) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let workspace_root = std::env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();

            let wave_path = workspace_root.join("test_data/ibex/sim.fst");
            let mapping_path = workspace_root.join("test_data/ibex/signal_get.py");
            let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

            let _ = dang::start_with_args_and_listener(wave_path, mapping_path, elf_path, listener);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;

    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_get_instructions() {
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(1000));

        // Connect with the client to actual dang instance
        let mut client = Client::new_with_port(port);
        sleep(Duration::from_millis(200)); // Increased delay for stability

        client
            .initialize_gdb_session()
            .expect("failed to init gdb session for test inst");
        sleep(Duration::from_millis(100)); // Increased delay for stability

        client.load_elf_info().expect("Failed to load elf info");

        let instructions = client
            .get_current_and_next_inst()
            .expect("Instructions not found");
        assert_ne!(instructions.len(), 0);

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    #[test]
    fn test_get_current_pc_method() {
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(1000));

        // Connect with the client to actual dang instance
        let mut client = Client::new_with_port(port);
        sleep(Duration::from_millis(200)); // Increased delay for stability

        // Initialize the client to ensure it's ready for commands
        match client.initialize_gdb_session() {
            Ok(_) => {
                // Add additional stabilization delay after initialization
                sleep(Duration::from_millis(100));

                match client.get_current_pc() {
                    Ok(pc) => {
                        // PC should be a reasonable 32-bit value
                        assert!(pc.nz(), "PC should be greater than 0");
                    }
                    Err(_e) => {
                        panic!("AHHHH FUCK WE CANT GET PC NOOOOOOO AAAHHH FILE AN ISSUE ! I WILL KNOW NO PEACE! FILE AN ISSUE ON GITHUB AND PLEASE BE NICE TO ME!");
                    }
                }
            }
            Err(e) => {
                panic!("Error initializing GDB session with real dang instance: {e}");
            }
        }

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    fn calculate_gdb_checksum(content: &str) -> String {
        let checksum = content.bytes().fold(0u8, |acc, b| acc.wrapping_add(b));
        format!("{checksum:02x}")
    }

    #[test]
    fn test_register_read_response_parsing_issue() {
        crate::init_test_logger();
        // Test that reproduces the jpdb "Unexpected response format for register read" issue
        use crate::response::GdbResponse;

        // Simulate various register response sizes that could cause parsing issues
        let test_cases = vec![
            // Case 1: Register data that's exactly 132 bytes (33 * 4-byte registers)
            vec![0x00; 132],
            // Case 2: Register data that's 130 bytes (not divisible by 4)
            vec![0x00; 130],
            // Case 3: Register data that's >128 bytes (fails the heuristic)
            vec![0x00; 136],
            // Case 4: Empty register data
            vec![],
            // Case 5: Very large register data
            vec![0x00; 256],
        ];

        for (i, register_data) in test_cases.iter().enumerate() {
            let hex_string = register_data
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();

            let checksum = calculate_gdb_checksum(&hex_string);
            let packet = format!("${hex_string}#{checksum}");
            let packet_type = Packet::Command(GdbCommand::Base(Base::LowerG));
            let response = GdbResponse::parse_packet(
                RawGdbResponse::find_packet_data(packet.as_bytes()).unwrap(),
                &packet_type,
            );

            log::info!(
                "Test case {}: data length = {}, result = {:?}",
                i,
                register_data.len(),
                response
            );

            match response {
                Ok(GdbResponse::RegisterData { data }) => {
                    assert_eq!(data.len(), register_data.len());
                    log::info!("✓ Correctly parsed as RegisterData");
                }
                Ok(GdbResponse::Empty) if register_data.is_empty() => {
                    log::info!("✓ Empty register data correctly parsed as Empty");
                }
                Ok(GdbResponse::MemoryData { data: _ }) => {
                    panic!("⚠ Parsed as MemoryData instead of RegisterData (length={}, divisible by 4={})", 
                        register_data.len(),
                        register_data.len() % 4 == 0);
                }
                Ok(other) => {
                    panic!("⚠ Parsed as unexpected type: {other:?}");
                }
                Err(e) => {
                    panic!("✗ Parse error: {e:?}");
                }
            }
        }
    }

    #[test]
    fn test_time_idx_command() {
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(1000));

        // Connect with the client to actual dang instance
        let mut client = Client::new_with_port(port);
        sleep(Duration::from_millis(200)); // Increased delay for stability

        client
            .initialize_gdb_session()
            .expect("failed to init gdb session for time_idx test");
        sleep(Duration::from_millis(100)); // Increased delay for stability

        // Send the time_idx monitor command
        let time_idx_output = client.get_time_idx();
        if let Err(e) = time_idx_output {
            panic!("err is {e:?} when getting time idx");
        }

        assert!(time_idx_output.is_ok());

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }
}
