use std::{
    io::{Read, Write},
    net::TcpStream,
};

use crate::{response::GdbResponse, Packet, commands::{GdbCommand, Base}};

pub struct Client {
    strm: TcpStream,
    packet_scratch: [u8; 4096],
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
}
