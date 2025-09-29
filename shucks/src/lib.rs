use std::io;

pub mod addr2line_stepper;
pub mod client;
pub mod commands;
pub mod packet;
pub mod response;
mod wavetracker;

pub use addr2line_stepper::SourceLine;
pub use client::Client;
pub use wellen::{TimeTableIdx, Var};
use commands::{Base, GdbCommand};
use packet::FinishedPacket;

/// Top-Level GDB packet
#[derive(Default, Debug)]
pub enum Packet {
    #[default]
    Ack,
    Command(GdbCommand),
}

impl Packet {
    fn ack() -> Result<FinishedPacket<'static>, io::Error> {
        Ok(FinishedPacket("+".as_bytes()))
    }

    pub fn is_memory_read(&self) -> bool {
        match self {
            Self::Ack => false,
            Self::Command(GdbCommand::Base(Base::LowerM { .. })) => true,
            Self::Command(_) => false,
        }
    }
    pub fn is_register_read(&self) -> bool {
        match self {
            Self::Ack => false,
            Self::Command(GdbCommand::Base(Base::LowerG)) => true,
            Self::Command(_) => false,
        }
    }

    pub fn is_monitor_command(&self) -> bool {
        match self {
            Self::Ack => false,
            Self::Command(GdbCommand::Base(Base::QRcmd { .. })) => true,
            Self::Command(_) => false,
        }
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

// Shared logger initialization for all tests
#[cfg(test)]
pub(crate) fn init_test_logger() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .is_test(true)
            .init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::test_utils::*;
    use crate::commands::{Base, GdbCommand, Resume};
    use std::{thread::sleep, time::Duration};

    #[test]
    fn sanity() {
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

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
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(1000));

        // Connect with the client
        let mut cl = Client::new_with_port(port);
        sleep(Duration::from_millis(100));

        cl.initialize_gdb_session().expect("Dog");

        // Step once
        let response1 = cl
            .send_command(&Packet::Command(GdbCommand::Resume(Resume::Step)))
            .expect("Failed to send first step command");
        log::info!(
            "First step response: {:?}",
            String::from_utf8_lossy(response1.as_slice())
        );

        // Step twice
        let response2 = cl
            .send_command(&Packet::Command(GdbCommand::Resume(Resume::Step)))
            .expect("Failed to send second step command");
        log::info!(
            "Second step response: {:?}",
            String::from_utf8_lossy(response2.as_slice())
        );

        sleep(Duration::from_millis(100));

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }

    #[test]
    fn gdb_initialization() {
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(1000));

        // Connect with the client
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
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(1000));

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

    #[test]
    fn test_get_executable_path() {
        crate::init_test_logger();
        let (listener, port) = create_test_listener();

        // Start dang GDB stub in a separate thread
        let handle = start_dang_instance(listener);

        // Give the server time to start
        sleep(Duration::from_millis(1000));

        // Connect with the client
        let mut cl = Client::new_with_port(port);
        sleep(Duration::from_millis(100));
        cl.initialize_gdb_session()
            .expect("Could not initialize gdb");

        // Test getting executable path
        let exec_path = cl
            .get_executable_path()
            .expect("Failed to get executable path");
        println!("Executable path: {exec_path}");

        // Verify the path contains our test ELF file
        assert!(
            exec_path.contains("hello_test.elf"),
            "Path should contain hello_test.elf"
        );

        sleep(Duration::from_millis(100));

        // Kill the handle by not waiting for it to complete
        drop(handle);
    }
}
