use libsurfer::wcp::proto::{WcpCSMessage, WcpCommand};
use num::BigInt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

/// WCP (Waveform Control Protocol) client for controlling Surfer waveform viewer
pub struct WcpClient {
    stream: TcpStream,
}

impl WcpClient {
    /// Connect to a WCP server at the given address
    pub fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;

        Ok(Self { stream })
    }

    /// Send a WCP command to the server
    fn send_command(&mut self, command: WcpCommand) -> Result<(), Box<dyn std::error::Error>> {
        let message = WcpCSMessage::command(command);
        let message_str = serde_json::to_string(&message)?;

        // Write message followed by null terminator (not newline!)
        self.stream.write_all(message_str.as_bytes())?;

        self.stream.flush()?;
        let mut buffer = Vec::new();
        self.stream.read(&mut buffer)?;
        log::info!("got response: {:?}", String::from_utf8_lossy(&buffer));

        Ok(())
    }

    /// Navigate to a specific timestamp in the waveform (time in picoseconds)
    pub fn goto_time(&mut self, time_ps: u64) -> Result<(), Box<dyn std::error::Error>> {
        let command = WcpCommand::set_viewport_to {
            timestamp: BigInt::from(time_ps),
        };
        let rv = self.send_command(command);
        log::info!("tried to set viewport to {time_ps} and got response: {rv:?}");
        rv
    }

    /// Add a signal to the waveform viewer
    pub fn add_signal(&mut self, signal_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let command = WcpCommand::add_variables {
            variables: vec![signal_path.to_string()],
        };
        self.send_command(command)
    }
}
