use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

/// WCP (Waveform Control Protocol) client for controlling Surfer waveform viewer
pub struct WcpClient {
    stream: TcpStream,
    next_id: u64,
}

impl WcpClient {
    /// Connect to a WCP server at the given address
    pub fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;

        Ok(Self {
            stream,
            next_id: 1,
        })
    }

    /// Send a JSON-RPC style command to the WCP server
    fn send_command(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let id = self.next_id;
        self.next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let request_str = serde_json::to_string(&request)?;
        writeln!(self.stream, "{}", request_str)?;
        self.stream.flush()?;

        // Read response
        let mut reader = BufReader::new(&self.stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        let response: serde_json::Value = serde_json::from_str(&response_line)?;

        // Check for error in response
        if let Some(error) = response.get("error") {
            return Err(format!("WCP error: {}", error).into());
        }

        Ok(response.get("result").cloned().unwrap_or(serde_json::Value::Null))
    }

    /// Load a waveform file
    pub fn load_waveform(&mut self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let params = serde_json::json!({
            "path": path.to_string_lossy()
        });

        self.send_command("loadWaveform", params)?;
        Ok(())
    }

    /// Navigate to a specific timestamp in the waveform
    pub fn goto_time(&mut self, time_ps: u64) -> Result<(), Box<dyn std::error::Error>> {
        let params = serde_json::json!({
            "time": time_ps
        });

        self.send_command("gotoTime", params)?;
        Ok(())
    }

    /// Add a signal to the waveform viewer
    pub fn add_signal(&mut self, signal_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let params = serde_json::json!({
            "signal": signal_path
        });

        self.send_command("addSignal", params)?;
        Ok(())
    }

    /// Set the cursor position to a specific time
    pub fn set_cursor(&mut self, time_ps: u64) -> Result<(), Box<dyn std::error::Error>> {
        let params = serde_json::json!({
            "time": time_ps
        });

        self.send_command("setCursor", params)?;
        Ok(())
    }
}