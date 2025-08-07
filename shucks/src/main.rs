use std::io;

use crate::{commands::GdbCommand, packet::FinishedPacket};

pub mod client;
pub mod commands;
pub mod packet;
pub mod response;

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

    pub fn to_finished_packet<'a>(
        &self,
        slice: &'a mut [u8],
    ) -> Result<FinishedPacket<'a>, io::Error> {
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
    use std::net::TcpListener;

    fn create_test_listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[test]
    fn sanity() {
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = thread::spawn(move || {
            let workspace_root = std::env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();

            let wave_path = workspace_root.join("test_data/ibex/sim.fst");
            let mapping_path = workspace_root.join("test_data/ibex/signal_get.py");
            let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

            dang::start_with_args_and_listener(wave_path, mapping_path, elf_path, listener)
                .expect("works");
        });

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client
        let _cl = Client::new_with_port(port);
        sleep(Duration::from_millis(300));

        // Kill the handle by not waiting for it to complete
        // The thread will be terminated when the test ends
        drop(handle);
    }

    #[test]
    fn step_twice() {
        use crate::commands::{Base, GdbCommand, Resume};

        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = thread::spawn(move || {
            let workspace_root = std::env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();

            let wave_path = workspace_root.join("test_data/ibex/sim.fst");
            let mapping_path = workspace_root.join("test_data/ibex/signal_get.py");
            let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

            dang::start_with_args_and_listener(wave_path, mapping_path, elf_path, listener)
                .expect("works");
        });

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client
        let mut cl = Client::new_with_port(port);
        sleep(Duration::from_millis(100));

        // Send initial query to establish connection
        let response = cl
            .send_command(Packet::Command(GdbCommand::Base(Base::QuestionMark)))
            .expect("Failed to send question mark command");
        println!(
            "Question mark response: {:?}",
            String::from_utf8_lossy(&response)
        );

        // Step once
        let response1 = cl
            .send_command(Packet::Command(GdbCommand::Resume(Resume::Step)))
            .expect("Failed to send first step command");
        println!(
            "First step response: {:?}",
            String::from_utf8_lossy(&response1)
        );

        // Step twice
        let response2 = cl
            .send_command(Packet::Command(GdbCommand::Resume(Resume::Step)))
            .expect("Failed to send second step command");
        println!(
            "Second step response: {:?}",
            String::from_utf8_lossy(&response2)
        );

        sleep(Duration::from_millis(100));

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    #[test]
    fn gdb_initialization() {
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = thread::spawn(move || {
            let workspace_root = std::env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();

            let wave_path = workspace_root.join("test_data/ibex/sim.fst");
            let mapping_path = workspace_root.join("test_data/ibex/signal_get.py");
            let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

            dang::start_with_args_and_listener(wave_path, mapping_path, elf_path, listener)
                .expect("works");
        });

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client and run GDB initialization sequence
        let mut cl = Client::new_with_port(port);
        sleep(Duration::from_millis(100));

        cl.initialize_gdb_session()
            .expect("Failed to initialize GDB session");

        sleep(Duration::from_millis(100));

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    #[test]
    fn test_parsed_responses() {
        use crate::commands::{Base, GdbCommand};

        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = thread::spawn(move || {
            let workspace_root = std::env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();

            let wave_path = workspace_root.join("test_data/ibex/sim.fst");
            let mapping_path = workspace_root.join("test_data/ibex/signal_get.py");
            let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

            dang::start_with_args_and_listener(wave_path, mapping_path, elf_path, listener)
                .expect("works");
        });

        // Give the server time to start
        sleep(Duration::from_millis(300));

        // Connect with the client
        let mut cl = Client::new_with_port(port);
        sleep(Duration::from_millis(100));

        // Test parsing various command responses
        let response = cl
            .send_command_parsed(Packet::Command(GdbCommand::Base(Base::QuestionMark)))
            .expect("Failed to send and parse question mark command");
        println!("Parsed ? response: {response}");

        let response = cl
            .send_command_parsed(Packet::Command(GdbCommand::Base(Base::QSupported)))
            .expect("Failed to send and parse qSupported command");
        println!("Parsed qSupported response: {response}");

        sleep(Duration::from_millis(100));

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }
}
