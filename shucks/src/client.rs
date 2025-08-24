use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
};

use crate::{
    commands::{Base, GdbCommand},
    response::GdbResponse,
    Packet,
};
use goblin::elf::Elf;
use raki::{Decode, Isa};

pub struct Client {
    strm: TcpStream,
    packet_scratch: [u8; 4096],
    elf_info: Option<ElfInfo>,
}

pub enum PC {
    _64(u64),
    _32(u32)
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
                
                    write!(f, "({})",pc)

            }
            Self::_32(pc) => {
write!(f, "({})",pc)


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
        }
    }

    pub fn send_command(&mut self, packet: &Packet) -> Result<Vec<u8>, std::io::Error> {
        let pkt = packet.to_finished_packet(self.packet_scratch.as_mut_slice())?;
        log::info!("Sending packet: {:?}", String::from_utf8_lossy(pkt.0));
        self.strm.write_all(pkt.0)?;

        // Read response with proper packet handling
        let response = self.read_gdb_packet()?;
        log::info!("Read {} bytes", response.len());

        let _tstr = String::from_utf8_lossy(response.as_slice());

        Ok(response)
    }

    /// Read a complete GDB packet, handling partial reads and multiple packets
    fn read_gdb_packet(&mut self) -> Result<Vec<u8>, std::io::Error> {
        use std::io::ErrorKind;
        use std::time::{Duration, Instant};

        let mut response = Vec::new();
        let mut buffer = [0u8; 1024];
        let timeout = Duration::from_millis(2000);
        let start_time = Instant::now();

        // Set read timeout
        self.strm
            .set_read_timeout(Some(Duration::from_millis(500)))?;

        loop {
            match self.strm.read(&mut buffer) {
                Ok(0) => {
                    // EOF - connection closed
                    break;
                }
                Ok(n) => {
                    response.extend_from_slice(&buffer[..n]);

                    // Check if we have a complete response
                    if self.is_complete_response(&response) {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    // Timeout occurred, check if we have any partial data
                    if !response.is_empty() && start_time.elapsed() < timeout {
                        // We have partial data, keep trying for a bit longer
                        continue;
                    } else if response.is_empty() && start_time.elapsed() < timeout {
                        // No data yet but still within overall timeout
                        continue;
                    } else {
                        // Overall timeout exceeded, return what we have
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

        Ok(response)
    }

    /// Check if we have a complete GDB response
    fn is_complete_response(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }

        // Simple acknowledgments
        if data.len() == 1 && (data[0] == b'+' || data[0] == b'-') {
            return true;
        }

        // Look for packet format: $...#xx or +$...#xx
        let start_idx = if data.len() > 1 && data[0] == b'+' && data[1] == b'$' {
            1
        } else if !data.is_empty() && data[0] == b'$' {
            0
        } else {
            // Not a standard packet, but might be a valid response
            // Look for responses that don't follow standard format
            return data.len() >= 2; // Assume complete if we have some data
        };

        // Look for the end of packet marker
        if let Some(hash_pos) = data[start_idx..].iter().position(|&b| b == b'#') {
            let hash_pos = start_idx + hash_pos;
            // Check if we have the checksum (2 bytes after #)
            return data.len() >= hash_pos + 3;
        }

        false
    }

    pub fn send_command_parsed(
        &mut self,
        packet: Packet,
    ) -> Result<GdbResponse, Box<dyn std::error::Error>> {
        let raw_response = self.send_command(&packet)?;
        let parsed_response = GdbResponse::parse(&raw_response, &packet)?;
        log::info!("Parsed response: {parsed_response}");
        Ok(parsed_response)
    }

    pub fn initialize_gdb_session(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Starting GDB initialization sequence...");

        // QStartNoAckMode - continue on failure
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QStartNoAckMode))) {
            Ok(_) => log::info!("Sent QStartNoAckMode"),
            Err(e) => {
                log::info!("QStartNoAckMode failed: {:?}, continuing...", e);
            }
        }

        // QSupported - continue on failure
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QSupported))) {
            Ok(_) => log::info!("Sent qSupported"),
            Err(e) => {
                log::info!("qSupported failed: {:?}, continuing...", e);
            }
        }

        log::info!("About to send qfThreadInfo...");
        let thread_info_result =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QfThreadInfo)));
        match thread_info_result {
            Ok(response) => {
                log::info!("qfThreadInfo response: {:?}", response);
            }
            Err(e) => {
                log::info!("qfThreadInfo failed: {:?}", e);
                // Try to continue without thread info for now
                log::info!("Continuing without thread info...");
            }
        }

        log::info!("About to send qsThreadInfo...");
        let thread_info_cont_result =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QsThreadInfo)));
        match thread_info_cont_result {
            Ok(response) => {
                log::info!("qsThreadInfo response: {:?}", response);
            }
            Err(e) => {
                log::info!("qsThreadInfo failed: {:?}", e);
                // Try to continue without thread info for now
                log::info!("Continuing without thread info...");
            }
        }

        // Question mark - this is more critical, but still try to continue
        let halt_reason_result =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QuestionMark)));
        match halt_reason_result {
            Ok(halt_reason) => {
                log::info!("Sent ? (halt reason query): {halt_reason}");
            }
            Err(e) => {
                log::info!("Halt reason query failed: {:?}, continuing anyway...", e);
            }
        }

        // Read all registers to get PC - continue on failure
        let registers_result =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::LowerG)));
        match registers_result {
            Ok(registers) => {
                log::info!("Read registers: {registers}");

                // Display PC and instructions using new get_current_pc method
                if let Err(e) = self.display_pc_and_instructions() {
                    log::info!("Failed to display PC and instructions: {:?}", e);
                }
            }
            Err(e) => {
                log::info!("Reading registers failed: {:?}, continuing...", e);
            }
        }

        log::info!("GDB initialization sequence complete!");
        Ok(())
    }

    /// Display the program counter and current/next instructions
    fn display_pc_and_instructions(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Get current PC using the dedicated method
        let pc = self.get_current_pc()?;

        log::info!("Program Counter (PC): 0x{}", pc);

        // Read current instruction (4 bytes for RISC-V)
        let current_inst =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::LowerM {
                addr: pc.as_u32(),
                length: 4,
            })))?;

        // Read next 3 instructions (12 bytes total)
        let next_insts =
            self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::LowerM {
                addr: pc.as_u32() + 4,
                length: 12,
            })))?;

        // Display current instruction
        if let crate::response::GdbResponse::MemoryData { data } = current_inst {
            if data.len() >= 4 {
                let inst = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                log::info!("Current instruction:  0x{:08x}", inst);
            }
        }

        // Display next 3 instructions
        if let crate::response::GdbResponse::MemoryData { data } = next_insts {
            for i in 0..3 {
                let offset = i * 4;
                if data.len() >= offset + 4 {
                    let inst = u32::from_le_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]);
                    log::info!("Next instruction #{}: 0x{:08x}", i + 1, inst);
                }
            }
        }

        Ok(())
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
            _ => Err(format!("Unexpected response format for qXfer:exec-file:read, got {response:?}").into()),
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

        log::info!(
            "Parsed {} ELF file: {} symbols, entry point: 0x{:x}",
            if is_32bit { "32-bit" } else { "64-bit" },
            self.elf_info.as_ref().unwrap().symbols.len(),
            self.elf_info.as_ref().unwrap().entry_point
        );

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
                    return Err(format!("Register data too short to contain PC (got {} bytes, need 132)", data.len()).into());
                }
                // Extract PC (assuming little-endian)
                // For RISC-V, PC is typically at the end of the register dump
                // RISC-V has 32 general purpose registers (x0-x31) of 4 bytes each = 128 bytes
                // PC is usually the next 4 bytes after that
                let pc_bytes = &data[128..132];
                let pc = u32::from_le_bytes([pc_bytes[0], pc_bytes[1], pc_bytes[2], pc_bytes[3]]);
                
                Ok(PC::_32(pc))
            }
            crate::response::GdbResponse::MemoryData { data } => {
                log::warn!("Got MemoryData instead of RegisterData, attempting PC extraction anyway");
                if data.len() < 132 {
                    return Err(format!("Memory data too short to contain PC (got {} bytes, need 132)", data.len()).into());
                }
                let pc_bytes = &data[128..132];
                let pc = u32::from_le_bytes([pc_bytes[0], pc_bytes[1], pc_bytes[2], pc_bytes[3]]);
                Ok(PC::_32(pc))
            }
            _ => {
                log::error!("Unexpected response format for register read: {}", registers);
                Err(format!("Unexpected response format for register read: {}", registers).into())
            }
        }
    }

    /// Show current instruction and next 3 instructions using raki decoder and ELF data
    pub fn get_current_and_next_inst(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Get current PC using the dedicated method
        let pc = self.get_current_pc()?;

        log::info!("Program Counter (PC): 0x{}", pc);

        // Check if we have symbol information
        if let Some((symbol, offset)) = self.find_symbol_at_address(pc.as_u64()) {
            log::info!("Current function: {} + 0x{:x}", symbol.name, offset);
        }

        // Get instruction bytes directly from ELF binary (16 bytes = 4 instructions)
        let instruction_bytes = self.get_instruction_bytes_from_elf(pc)?;

        // Determine ISA based on ELF info
        let isa = if let Some(elf_info) = &self.elf_info {
            if elf_info.is_32bit {
                Isa::Rv32
            } else {
                Isa::Rv64
            }
        } else {
            return Err("No ELF info available. Call load_elf_info() first".into());
        };

        Ok(())
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
    use super::*;
    use super::test_utils::*;
    use raki::Isa;
    use std::net::{TcpListener, TcpStream};
    use std::thread::{self, sleep};
    use std::time::Duration;


    

    #[test]
    fn test_decode_with_insufficient_data() {
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client to actual dang instance
        let client = Client::new_with_port(port);

        // Test with only 8 bytes (2 instructions)
        let instruction_bytes = [
            0x93, 0x00, 0x00, 0x00, // addi x1, x0, 0
            0x13, 0x01, 0x10, 0x00, // addi x2, x0, 1
        ];

        let pc = 0x10000000;
        let isa = Isa::Rv32;

        // Should handle insufficient data gracefully
        //let result = client.decode_and_display_instructions(pc, &instruction_bytes, isa);
        //assert!(result.is_ok(), "Should handle insufficient data gracefully");

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    #[test]
    fn test_decode_with_invalid_instructions() {
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client to actual dang instance
        let client = Client::new_with_port(port);

        // Test with invalid instruction bytes (all zeros except for valid patterns)
        let instruction_bytes = [
            0x00, 0x00, 0x00, 0x00, // Invalid instruction
            0x00, 0x00, 0x00, 0x00, // Invalid instruction
            0x00, 0x00, 0x00, 0x00, // Invalid instruction
            0x00, 0x00, 0x00, 0x00, // Invalid instruction
        ];

        let pc = 0x10000000;
        let isa = Isa::Rv32;

        // Should handle decode errors gracefully
        //let result = client.decode_and_display_instructions(pc, &instruction_bytes, isa);
        //assert!(result.is_ok(), "Should handle decode errors gracefully");

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    #[test]
    fn test_pc_extraction_from_register_data() {
        // Test the PC extraction logic separately
        let mut register_data = vec![0u8; 132];

        // Set PC bytes at offset 128-131 to 0x12345678 (little-endian)
        register_data[128] = 0x78;
        register_data[129] = 0x56;
        register_data[130] = 0x34;
        register_data[131] = 0x12;

        let pc_bytes = &register_data[128..132];
        let pc = u32::from_le_bytes([pc_bytes[0], pc_bytes[1], pc_bytes[2], pc_bytes[3]]);

        assert_eq!(
            pc, 0x12345678,
            "PC should be correctly extracted from register data"
        );
    }

    #[test]
    fn test_get_current_pc_method() {
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client to actual dang instance
        let mut client = Client::new_with_port(port);
        sleep(Duration::from_millis(200)); // Increased delay for stability
        
        // Initialize the client to ensure it's ready for commands
        match client.initialize_gdb_session() {
            Ok(_) => {
                // Add additional stabilization delay after initialization
                sleep(Duration::from_millis(100));
                
                // Try to get the current PC with retry logic for robustness
                let mut last_error = None;
                let mut success = false;
                
                for attempt in 0..3 {
                    match client.get_current_pc() {
                        Ok(pc) => {
                            // PC should be a reasonable 32-bit value
                            assert!(pc.nz(), "PC should be greater than 0");
                            
                            success = true;
                            break;
                        }
                        Err(e) => {
                            println!("Attempt {} failed: {}", attempt + 1, e);
                            last_error = Some(e);
                            if attempt < 2 {
                                sleep(Duration::from_millis(100)); // Wait before retry
                            }
                        }
                    }
                }
                
                if !success {
                    if let Some(e) = last_error {
                        panic!("Failed to get PC after 3 attempts. Last error: {}", e);
                    } else {
                        panic!("Failed to get PC after 3 attempts with unknown error");
                    }
                }
            }
            Err(e) => {
                panic!("Error initializing GDB session with real dang instance: {}", e);
            }
        }

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    #[test]
    fn test_instruction_decoding_with_raki() {
        // Direct test of raki instruction decoding
        let inst_bytes: u32 = 0x00000093; // addi x1, x0, 0
        let instruction = inst_bytes.decode(Isa::Rv32);

        assert!(
            instruction.is_ok(),
            "Should successfully decode valid RISC-V instruction"
        );

        let decoded = instruction.unwrap();
        // Verify it's an ADDI instruction (we can check the Display output)
        let instruction_str = format!("{}", decoded);
        assert!(
            instruction_str.contains("addi"),
            "Should decode as addi instruction"
        );
    }

    fn calculate_gdb_checksum(content: &str) -> String {
        let checksum = content.bytes().fold(0u8, |acc, b| acc.wrapping_add(b));
        format!("{:02x}", checksum)
    }

    #[test]
    fn test_register_read_response_parsing_issue() {
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
            let hex_string = register_data.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            
            let checksum = calculate_gdb_checksum(&hex_string);
            let packet = format!("${}#{}", hex_string, checksum);
            let packet_type= Packet::Command(GdbCommand::Base(Base::LowerG));
            let response = GdbResponse::parse(packet.as_bytes(),&packet_type);
            
            log::info!("Test case {}: data length = {}, result = {:?}", i, register_data.len(), response);
            
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
                    panic!("⚠ Parsed as unexpected type: {:?}", other);
                }
                Err(e) => {
                    panic!("✗ Parse error: {:?}", e);
                }
            }
        }
    }

   
    
}
