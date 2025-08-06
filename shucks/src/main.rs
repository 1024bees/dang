use std::io;

use crate::{
    commands::GdbCommand,
    packet::FinishedPacket,
};

pub mod client;
pub mod commands;
pub mod packet;

pub use client::Client;

/// Top-Level GDB packet
pub enum Packet {
    Ack,
    //Nack,
    //Interrupt,
    Command(GdbCommand),
}

impl Packet {
    fn ack() -> Result<FinishedPacket<'static>, io::Error> {
        Ok(FinishedPacket("+".as_bytes()))
    }

    pub fn to_finished_packet<'a>(&self, slice: &'a mut [u8]) -> Result<FinishedPacket<'a>, io::Error> {
        let rv = match self {
            Self::Ack => Packet::ack(),
            Self::Command(command) => command.to_command(slice),
        };
        rv
    }
}

fn main() {
    println!("Hello, world!");
    let cl = Client::new();
}

#[cfg(test)]
mod tests {
    use std::{
        thread::{self, sleep},
        time::Duration,
    };

    use super::*;

    #[test]
    fn sanity() {
        // Start dang GDB stub in a separate thread
        let handle = thread::spawn(|| {
            let workspace_root = std::env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();

            let wave_path = workspace_root.join("test_data/ibex/sim.fst");
            let mapping_path = workspace_root.join("test_data/ibex/signal_get.py");
            let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

            dang::start_with_args(wave_path, mapping_path, elf_path).expect("works");
        });

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client
        let _cl = Client::new();
        sleep(Duration::from_millis(300));

        // Kill the handle by not waiting for it to complete
        // The thread will be terminated when the test ends
        drop(handle);
    }
}
