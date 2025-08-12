use std::{
    io::{Read, Write},
    net::TcpStream,
    fs,
};

use goblin::elf::Elf;
use crate::{response::GdbResponse, Packet, commands::{GdbCommand, Base}};

pub struct Client {
    strm: TcpStream,
    packet_scratch: [u8; 4096],
    elf_info: Option<ElfInfo>,
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

    pub fn send_command(&mut self, packet: Packet) -> Result<Vec<u8>, std::io::Error> {
        let pkt = packet.to_finished_packet(self.packet_scratch.as_mut_slice())?;
        println!("Sending packet: {:?}", String::from_utf8_lossy(pkt.0));
        self.strm.write_all(pkt.0)?;

        // Read response with proper packet handling
        let response = self.read_gdb_packet()?;
        println!("Read {} bytes", response.len());

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
        self.strm.set_read_timeout(Some(Duration::from_millis(500)))?;
        
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
        let raw_response = self.send_command(packet)?;
        let parsed_response = GdbResponse::parse(&raw_response)?;
        println!("Parsed response: {parsed_response}");
        Ok(parsed_response)
    }

    pub fn initialize_gdb_session(&mut self) -> Result<(), Box<dyn std::error::Error>> {

        println!("Starting GDB initialization sequence...");

        // QStartNoAckMode - continue on failure
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QStartNoAckMode))) {
            Ok(_) => println!("Sent QStartNoAckMode"),
            Err(e) => {
                println!("QStartNoAckMode failed: {:?}, continuing...", e);
            }
        }

        // QSupported - continue on failure
        match self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QSupported))) {
            Ok(_) => println!("Sent qSupported"),
            Err(e) => {
                println!("qSupported failed: {:?}, continuing...", e);
            }
        }

        println!("About to send qfThreadInfo...");
        let thread_info_result = self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QfThreadInfo)));
        match thread_info_result {
            Ok(response) => {
                println!("qfThreadInfo response: {:?}", response);
            },
            Err(e) => {
                println!("qfThreadInfo failed: {:?}", e);
                // Try to continue without thread info for now
                println!("Continuing without thread info...");
            }
        }

        println!("About to send qsThreadInfo...");
        let thread_info_cont_result = self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QsThreadInfo)));
        match thread_info_cont_result {
            Ok(response) => {
                println!("qsThreadInfo response: {:?}", response);
            },
            Err(e) => {
                println!("qsThreadInfo failed: {:?}", e);
                // Try to continue without thread info for now
                println!("Continuing without thread info...");
            }
        }

        // Question mark - this is more critical, but still try to continue
        let halt_reason_result = self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::QuestionMark)));
        match halt_reason_result {
            Ok(halt_reason) => {
                println!("Sent ? (halt reason query): {halt_reason}");
            },
            Err(e) => {
                println!("Halt reason query failed: {:?}, continuing anyway...", e);
            }
        }

        // Read all registers to get PC - continue on failure
        let registers_result = self.send_command_parsed(Packet::Command(GdbCommand::Base(Base::LowerG)));
        match registers_result {
            Ok(registers) => {
                println!("Read registers: {registers}");
                
                // Extract PC from registers and read current instruction + next 3
                if let crate::response::GdbResponse::RegisterData { data } = registers {
                    if let Err(e) = self.display_pc_and_instructions(&data) {
                        println!("Failed to display PC and instructions: {:?}", e);
                    }
                }
            },
            Err(e) => {
                println!("Reading registers failed: {:?}, continuing...", e);
            }
        }

        println!("GDB initialization sequence complete!");
        Ok(())
    }

    /// Display the program counter and current/next instructions
    fn display_pc_and_instructions(&mut self, register_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // For RISC-V, PC is typically at the end of the register dump
        // RISC-V has 32 general purpose registers (x0-x31) of 4 bytes each = 128 bytes
        // PC is usually the next 4 bytes after that
        if register_data.len() < 132 {
            println!("Register data too short to contain PC");
            return Ok(());
        }

        // Extract PC (assuming little-endian)
        let pc_bytes = &register_data[128..132];
        let pc = u32::from_le_bytes([pc_bytes[0], pc_bytes[1], pc_bytes[2], pc_bytes[3]]);
        
        println!("Program Counter (PC): 0x{:08x}", pc);

        // Read current instruction (4 bytes for RISC-V)
        let current_inst = self.send_command_parsed(
            Packet::Command(GdbCommand::Base(Base::LowerM { addr: pc, length: 4 }))
        )?;
        
        // Read next 3 instructions (12 bytes total)
        let next_insts = self.send_command_parsed(
            Packet::Command(GdbCommand::Base(Base::LowerM { addr: pc + 4, length: 12 }))
        )?;

        // Display current instruction
        if let crate::response::GdbResponse::MemoryData { data } = current_inst {
            if data.len() >= 4 {
                let inst = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                println!("Current instruction:  0x{:08x}", inst);
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
                        data[offset + 3]
                    ]);
                    println!("Next instruction #{}: 0x{:08x}", i + 1, inst);
                }
            }
        }

        Ok(())
    }

    /// Get the executable file path from the remote target
    pub fn get_executable_path(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let response = self.send_command_parsed(
            Packet::Command(GdbCommand::Base(Base::QXferExecFile { 
                offset: 0, 
                length: 1000 
            }))
        )?;

        match response {
            crate::response::GdbResponse::QXferData { data, .. } => {
                // The data should contain the executable path as a string
                let path = String::from_utf8(data)?;
                Ok(path)
            }
            _ => Err("Unexpected response format for qXfer:exec-file:read".into()),
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
            return Err(format!("Not a RISC-V binary (machine type: 0x{:x})", elf.header.e_machine).into());
        }

        // Find .text section
        let text_section = elf.section_headers
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

        println!("Parsed {} ELF file: {} symbols, entry point: 0x{:x}", 
                 if is_32bit { "32-bit" } else { "64-bit" },
                 self.elf_info.as_ref().unwrap().symbols.len(),
                 self.elf_info.as_ref().unwrap().entry_point);

        Ok(())
    }

    /// Get 12 bytes of instruction data from ELF file starting at given PC
    pub fn get_instruction_bytes_from_elf(&self, pc: u32) -> Result<[u8; 12], Box<dyn std::error::Error>> {
        let elf_info = self.elf_info.as_ref()
            .ok_or("No ELF file loaded. Call parse_elf_file() first")?;

        let text_section = elf_info.text_section.as_ref()
            .ok_or("No .text section found in ELF file")?;

        // Check if PC is within .text section bounds
        let pc_u64 = pc as u64;
        if pc_u64 < text_section.addr || pc_u64 >= text_section.addr + text_section.size {
            return Err(format!("PC 0x{:x} is outside .text section (0x{:x}-0x{:x})", 
                              pc, text_section.addr, text_section.addr + text_section.size).into());
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
}
