use libsurfer::wcp::proto::{WcpCSMessage, WcpCommand};
use num::BigInt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

/// WCP (Waveform Control Protocol) client for controlling Surfer waveform viewer
pub struct WcpClient {
    stream: TcpStream,
}

impl WcpClient {
    /// Connect to a WCP server at the given address
    pub fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;

        // Set read timeout to 10 seconds
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;

        let mut rv = Self { stream };
        rv.greet()?;
        Ok(rv)
    }

    fn greet(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let message = WcpCSMessage::greeting {
            version: "0".to_string(),
            commands: vec!["set_viewport_to".to_string()],
        };
        self.send_message(message)?;
        log::info!("greeted surfer");
        Ok(())
    }

    fn send_message(&mut self, message: WcpCSMessage) -> Result<(), Box<dyn std::error::Error>> {
        let message_str = serde_json::to_string(&message)?;

        // Debug: log the JSON being sent
        log::info!("Sending WCP message: {}", message_str);

        // Write message followed by null terminator (not newline!)
        self.stream.write_all(message_str.as_bytes())?;
        self.stream.write_all(b"\0")?;
        self.stream.flush()?;

        // Read response until null terminator (with timeout)
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];

        loop {
            match self.stream.read_exact(&mut byte) {
                Ok(_) => {
                    if byte[0] == 0 {
                        break; // Found null terminator
                    }
                    buffer.push(byte[0]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    return Err(format!("Timeout waiting for response from WCP server").into());
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err(format!(
                        "Server closed connection unexpectedly. Partial response: {:?}",
                        String::from_utf8_lossy(&buffer)
                    )
                    .into());
                }
                Err(e) => return Err(e.into()),
            }
        }

        log::info!("got response: {:?}", String::from_utf8_lossy(&buffer));
        Ok(())
    }

    /// Send a WCP command to the server
    fn send_command(&mut self, command: WcpCommand) -> Result<(), Box<dyn std::error::Error>> {
        let message = WcpCSMessage::command(command);
        self.send_message(message)
    }

    /// Navigate to a specific timestamp in the waveform (time in picoseconds)
    pub fn goto_time(&mut self, time_ps: u64) -> Result<(), Box<dyn std::error::Error>> {
        //TODO: this looks to be buggy on the surfer side, from what i can tell.
        //let command = WcpCommand::set_viewport_to {
        //    timestamp: BigInt::from(time_ps),
        //};
        //let rv = self.send_command(command);
        //log::info!(
        //    "tried to set viewport to {time_ps}\n\
        //     and got response: \n\
        //     {rv:?}"
        //);
        Ok(())
        //rv
    }

    /// Add a signal to the waveform viewer
    pub fn add_signal(&mut self, signal_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let command = WcpCommand::add_variables {
            variables: vec![signal_path.to_string()],
        };
        self.send_command(command)
    }
}
