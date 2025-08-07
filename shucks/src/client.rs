use std::{
    io::{Read, Write},
    net::TcpStream,
};

use crate::{
    response::GdbResponse,
    Packet,
};

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

        let mut buffer = [0u8; 4096];
        let bytes_read = self.strm.read(&mut buffer)?;

        println!("Read {bytes_read} bytes");

        let rv = buffer[..bytes_read].to_vec();

        let tstr = String::from_utf8_lossy(rv.as_slice());

        Ok(rv)
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

    pub fn initialize_gdb_session(&mut self) -> Result<(), std::io::Error> {
        use crate::commands::{Base, GdbCommand};

        println!("Starting GDB initialization sequence...");

        self.send_command(Packet::Command(GdbCommand::Base(Base::QStartNoAckMode)))?;
        println!("Sent QStartNoAckMode");

        self.send_command(Packet::Command(GdbCommand::Base(Base::QSupported)))?;
        println!("Sent qSupported");

        self.send_command(Packet::Command(GdbCommand::Base(Base::QfThreadInfo)))?;
        println!("Sent qfThreadInfo");

        self.send_command(Packet::Command(GdbCommand::Base(Base::QsThreadInfo)))?;
        println!("Sent qsThreadInfo");

        self.send_command(Packet::Command(GdbCommand::Base(Base::QuestionMark)))?;
        println!("Sent ? (halt reason query)");

        println!("GDB initialization sequence complete!");
        Ok(())
    }
}
