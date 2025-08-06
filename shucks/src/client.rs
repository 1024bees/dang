use std::{
    io::{Read, Write},
    net::TcpStream,
};

use crate::Packet;

pub struct Client {
    strm: TcpStream,
    packet_scratch: [u8; 200],
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        let addr = "127.0.0.1:9001";
        let strm = TcpStream::connect(addr).unwrap();
        strm.set_nodelay(true).unwrap();
        Self {
            strm,
            packet_scratch: [0; 200],
        }
    }

    pub fn send_command(&mut self, packet: Packet) -> Result<Vec<u8>, std::io::Error> {
        let pkt = packet.to_finished_packet(self.packet_scratch.as_mut_slice())?;
        self.strm.write_all(pkt.0)?;
        let mut rv = Vec::with_capacity(1024);
        let read = self.strm.read(rv.as_mut_slice())?;

        dbg!("read {} bytes", read);

        Ok(rv)
    }
}