use std::{fmt, str};

use crate::Packet;

/// Represents the different types of responses from a GDB stub server
#[derive(Debug, Clone, PartialEq)]
pub enum GdbResponse {
    /// Acknowledgment responses
    Ack, // '+'
    Nack, // '-'

    /// Simple status responses  
    Ok, // "OK"
    Empty, // Empty response for unsupported commands

    /// Error responses
    Error {
        code: u8,
    }, // "Exx" where xx is hex error code

    /// Stop reply packets - indicate why target halted
    StopReply {
        signal: u8,
        thread_id: Option<ThreadId>,
        reason: StopReason,
    },

    /// Memory read response - hex-encoded data
    MemoryData {
        data: Vec<u8>,
    },

    /// Register read response - hex-encoded register values  
    RegisterData {
        data: Vec<u8>,
    },

    /// Thread info responses
    ThreadInfo {
        threads: Vec<ThreadId>,
        more_data: bool, // true if this is partial data (qfThreadInfo vs qsThreadInfo)
    },

    /// qSupported response - feature negotiation
    Supported {
        features: Vec<String>,
    },

    /// qXfer response - for transferring special data
    QXferData {
        data: Vec<u8>,
        is_final: bool, // true if this is the last chunk (starts with 'l'), false if more data ('m')
    },

    /// Binary data with run-length encoding support
    BinaryData {
        data: Vec<u8>,
    },

    /// Monitor command output (qRcmd responses)
    MonitorOutput {
        output: String,
    },

    /// Raw packet data for unrecognized responses
    Raw {
        data: Vec<u8>,
    },
}

/// Thread ID representation
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadId {
    Any,                            // 0
    All,                            // -1
    Specific(u32),                  // positive number
    Process { pid: u32, tid: u32 }, // process.thread for multiprocess
}

/// Reasons why the target stopped
#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    Signal(u8),
    Breakpoint,
    Watchpoint { addr: u32 },
    SingleStep,
    ProcessExit { code: u8 },
    Unknown,
}

/// Error type for response parsing
#[derive(Debug)]
pub enum ParseError {
    InvalidFormat,
    InvalidChecksum,
    InvalidHex,
    IncompletePacket,
    IoError(std::io::Error),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidFormat => write!(f, "Invalid packet format"),
            ParseError::InvalidChecksum => write!(f, "Invalid checksum"),
            ParseError::InvalidHex => write!(f, "Invalid hexadecimal data"),
            ParseError::IncompletePacket => write!(f, "Incomplete packet"),
            ParseError::IoError(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        ParseError::IoError(err)
    }
}

impl GdbResponse {
    /// Parse raw response bytes into a structured GdbResponse
    pub fn parse(raw_data: &[u8], packet: &Packet) -> Result<Self, ParseError> {
        log::debug!(
            "Parsing raw data ({} bytes): {:?}",
            raw_data.len(),
            String::from_utf8_lossy(raw_data)
        );

        if raw_data.is_empty() {
            log::debug!("Empty data -> GdbResponse::Empty");
            return Ok(GdbResponse::Empty);
        }

        // Handle acknowledgment responses
        if raw_data.len() == 1 {
            match raw_data[0] {
                b'+' => {
                    log::debug!("Parsed as ACK");
                    return Ok(GdbResponse::Ack);
                }
                b'-' => {
                    log::debug!("Parsed as NACK");
                    return Ok(GdbResponse::Nack);
                }
                _ => {} // Fall through to packet parsing
            }
        }

        // Handle mixed ACK + packet response (e.g., "+$OK#9a")
        if raw_data.len() > 1 && raw_data[0] == b'+' && raw_data[1] == b'$' {
            // Skip the ACK and parse the packet part
            return Self::parse_packet(&raw_data[1..], packet);
        }

        // Handle packet responses (format: $content#checksum)
        if raw_data[0] == b'$' {
            return Self::parse_packet(raw_data, packet);
        }

        // Handle malformed packets that are missing the '$' prefix (e.g., "OK#9a")
        if raw_data.len() >= 4 {
            if let Some(hash_pos) = raw_data.iter().rposition(|&b| b == b'#') {
                if hash_pos + 3 == raw_data.len() {
                    // This looks like a packet without the '$' prefix, add it and parse
                    let mut fixed_packet = vec![b'$'];
                    fixed_packet.extend_from_slice(raw_data);
                    return Self::parse_packet(&fixed_packet, packet);
                }
            }
        }

        // Handle other simple responses that may not be packets
        if raw_data == b"OK" {
            return Ok(GdbResponse::Ok);
        }

        // Handle special cases for thread info responses
        // "9a" might be a checksum for "$l#XX" (end of thread list)
        if raw_data.len() == 2 && Self::is_hex_data(raw_data) {
            // This might be a checksum for an "l" response, treat as end of thread list
            return Ok(GdbResponse::ThreadInfo {
                threads: vec![],
                more_data: false,
            });
        }

        // If it doesn't match expected formats, return as raw data
        Ok(GdbResponse::Raw {
            data: raw_data.to_vec(),
        })
    }

    /// Parse a GDB packet (starting with '$' and ending with '#xx')
    fn parse_packet(data: &[u8], packet: &Packet) -> Result<Self, ParseError> {
        if data.len() < 4 || data[0] != b'$' {
            return Err(ParseError::InvalidFormat);
        }

        // Find the '#' separator - use position instead of rposition to get the first one
        let hash_pos = data
            .iter()
            .position(|&b| b == b'#')
            .ok_or(ParseError::InvalidFormat)?;

        // We need at least 2 more characters after # for checksum
        if hash_pos + 3 > data.len() {
            // Try parsing without checksum verification
            let content = &data[1..hash_pos];
            return Self::parse_content(content, packet);
        }

        let content = &data[1..hash_pos];
        // Extract exactly 2 characters for checksum, ignore anything after
        let checksum_str = str::from_utf8(&data[hash_pos + 1..hash_pos + 3])
            .map_err(|_| ParseError::InvalidFormat)?;

        // Verify checksum
        let expected_checksum =
            u8::from_str_radix(checksum_str, 16).map_err(|_| ParseError::InvalidChecksum)?;

        let actual_checksum = content.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));

        if actual_checksum != expected_checksum {
            return Err(ParseError::InvalidChecksum);
        }

        Self::parse_content(content, packet)
    }

    /// Parse the content portion of a GDB packet
    fn parse_content(content: &[u8], packet: &Packet) -> Result<Self, ParseError> {
        println!("parsing response to packet {packet:?}, data is {content:?}");
        log::debug!(
            "Parsing content ({} bytes): {:?}",
            content.len(),
            String::from_utf8_lossy(content)
        );

        if content.is_empty() {
            log::debug!("Empty content -> GdbResponse::Empty");
            return Ok(GdbResponse::Empty);
        }

        let content_str = str::from_utf8(content).unwrap_or(""); // Allow non-UTF8 for binary data

        match content {
            // Simple OK response
            b"OK" => Ok(GdbResponse::Ok),

            // Error response (Exx)
            content if content.len() >= 3 && content[0] == b'E' => {
                let code_str =
                    str::from_utf8(&content[1..3]).map_err(|_| ParseError::InvalidHex)?;
                let code = u8::from_str_radix(code_str, 16).map_err(|_| ParseError::InvalidHex)?;
                Ok(GdbResponse::Error { code })
            }

            // Stop reply packet (Sxx or Txx...)
            content if content.len() >= 3 && (content[0] == b'S' || content[0] == b'T') => {
                Self::parse_stop_reply(content)
            }

            // qXfer responses (m<data> or l<data>)
            content if content.starts_with(b"m") => {
                // This could be either thread info or qXfer data
                // Try to parse as thread info first, then fall back to qXfer
                if Self::looks_like_thread_info(&content[1..]) {
                    Self::parse_thread_info(content, false)
                } else {
                    // Parse as qXfer data
                    Ok(GdbResponse::QXferData {
                        data: content[1..].to_vec(),
                        is_final: false,
                    })
                }
            }
            content if content.starts_with(b"l") => {
                // This could be end of thread info or final qXfer data
                if content.len() == 1 {
                    // Just 'l' - end of thread info
                    Ok(GdbResponse::ThreadInfo {
                        threads: vec![],
                        more_data: false,
                    })
                } else {
                    // 'l' followed by data - final qXfer chunk
                    Ok(GdbResponse::QXferData {
                        data: content[1..].to_vec(),
                        is_final: true,
                    })
                }
            }
            // Handle raw thread info responses that might not be properly formatted
            content if content.len() == 2 && Self::is_hex_data(content) => {
                // This might be a malformed thread info response, treat as end of thread list
                Ok(GdbResponse::ThreadInfo {
                    threads: vec![],
                    more_data: false,
                })
            }

            // qSupported response
            _content
                if content_str.contains("PacketSize")
                    || content_str.contains("qRelocInsn")
                    || content_str.contains("swbreak") =>
            {
                Self::parse_supported_response(content_str)
            }

            // Monitor command responses (typically for qRcmd)
            content if packet.is_monitor_command() => {
                // Monitor responses often come in the format "O<hex-encoded-text>"
                // where 'O' indicates console output
                let output = if content.starts_with(b"O") && content.len() > 1 {
                    // Strip the 'O' prefix and decode the remaining hex
                    println!("stripped hex content is {:?}", &content[1..]);
                    let hex_content = &content[1..];
                    if Self::is_hex_data(hex_content) {
                        println!("hex content is hex data");
                        match Self::decode_hex(hex_content) {
                            Ok(decoded_bytes) => {
                                println!("decoded bytes are {:?}", &decoded_bytes);
                                String::from_utf8_lossy(&decoded_bytes).to_string()
                            }
                            Err(e) => {
                                println!("error is {e:?} when decoding hex data");
                                String::from_utf8_lossy(content).to_string()
                            }
                        }
                    } else {
                        String::from_utf8_lossy(content).to_string()
                    }
                } else if Self::is_hex_data(content) {
                    println!("hex content is hex data");
                    println!("unstripped hex content is {:?}", &content[1..]);

                    // Try to decode as hex first
                    match Self::decode_hex(content) {
                        Ok(decoded_bytes) => {
                            println!("decoded bytes are {:?}", &decoded_bytes);
                            String::from_utf8_lossy(&decoded_bytes).to_string()
                        }
                        Err(e) => {
                            println!("error is {e:?} when decoding hex data");
                            String::from_utf8_lossy(content).to_string()
                        }
                    }
                } else {
                    String::from_utf8_lossy(content).to_string()
                };
                Ok(GdbResponse::MonitorOutput { output })
            }

            // Hex-encoded data (register or memory reads) - always try run-length decoding first
            content if Self::is_hex_data_or_run_length(content) => {
                // Always decode run-length encoding first, then hex
                let run_length_decoded = Self::decode_run_length(content);
                let data = Self::decode_hex(&run_length_decoded)?;

                log::info!(
                    "Original content was {:?}",
                    String::from_utf8_lossy(content)
                );
                log::info!("Decoded run-length + hex data: {} bytes", data.len());

                // Use packet type to determine response classification
                if packet.is_register_read() {
                    log::debug!(
                        "Classified as RegisterData based on packet type (length={})",
                        data.len()
                    );
                    Ok(GdbResponse::RegisterData { data })
                } else if packet.is_memory_read() {
                    log::debug!(
                        "Classified as MemoryData based on packet type (length={})",
                        data.len()
                    );
                    Ok(GdbResponse::MemoryData { data })
                } else {
                    // Fallback: use heuristic for unknown packet types
                    if data.len() >= 128 && data.len() % 4 == 0 {
                        log::debug!(
                            "Heuristically classified as RegisterData (length={}, divisible by 4)",
                            data.len()
                        );
                        Ok(GdbResponse::RegisterData { data })
                    } else {
                        log::debug!(
                            "Classified as Raw data (unknown packet type, length={})",
                            data.len()
                        );
                        Ok(GdbResponse::Raw { data })
                    }
                }
            }

            // Default: return as raw data
            _ => {
                log::debug!("Classified as Raw data (no specific pattern matched)");
                Ok(GdbResponse::Raw {
                    data: content.to_vec(),
                })
            }
        }
        .map(|response| {
            log::debug!("Final parsed response: {response}");
            response
        })
    }

    /// Parse stop reply packets (S or T packets)
    fn parse_stop_reply(content: &[u8]) -> Result<Self, ParseError> {
        if content.len() < 3 {
            return Err(ParseError::InvalidFormat);
        }

        let signal_str = str::from_utf8(&content[1..3]).map_err(|_| ParseError::InvalidHex)?;
        let signal = u8::from_str_radix(signal_str, 16).map_err(|_| ParseError::InvalidHex)?;

        // For now, we'll parse just the basic signal
        // TODO: Parse additional stop reply information (thread ID, registers, etc.)
        Ok(GdbResponse::StopReply {
            signal,
            thread_id: None,
            reason: StopReason::Signal(signal),
        })
    }

    /// Parse thread info responses (mXX,YY,ZZ...)
    fn parse_thread_info(content: &[u8], more_data: bool) -> Result<Self, ParseError> {
        if content.len() < 2 || content[0] != b'm' {
            return Err(ParseError::InvalidFormat);
        }

        let thread_list_str =
            str::from_utf8(&content[1..]).map_err(|_| ParseError::InvalidFormat)?;

        let mut threads = Vec::new();

        for thread_str in thread_list_str.split(',') {
            if thread_str == "0" {
                threads.push(ThreadId::Any);
            } else if thread_str == "-1" {
                threads.push(ThreadId::All);
            } else if let Ok(tid) = thread_str.parse::<u32>() {
                threads.push(ThreadId::Specific(tid));
            }
            // TODO: Handle process.thread format
        }

        Ok(GdbResponse::ThreadInfo { threads, more_data })
    }

    /// Parse qSupported response
    fn parse_supported_response(content: &str) -> Result<Self, ParseError> {
        let features: Vec<String> = content.split(';').map(|s| s.to_string()).collect();

        Ok(GdbResponse::Supported { features })
    }

    /// Check if content appears to be hexadecimal data
    fn is_hex_data(content: &[u8]) -> bool {
        !content.is_empty()
            && content.iter().all(|&b| {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b)
            })
    }

    /// Check if content appears to be hexadecimal data or contains run-length encoding
    fn is_hex_data_or_run_length(content: &[u8]) -> bool {
        if content.is_empty() {
            return false;
        }

        // First check if it's pure hex data
        if Self::is_hex_data(content) {
            return true;
        }

        // Check if it contains run-length encoding patterns
        let mut i = 0;
        while i < content.len() {
            if i + 2 < content.len() && content[i + 1] == b'*' {
                // Found a potential run-length pattern: char + '*' + count
                let repeated_char = content[i];
                let repeat_count_char = content[i + 2];

                // Verify the repeated char is hex and count is valid (>= 29)
                if (repeated_char.is_ascii_digit()
                    || (b'a'..=b'f').contains(&repeated_char)
                    || (b'A'..=b'F').contains(&repeated_char))
                    && repeat_count_char >= 29
                {
                    i += 3; // Skip this valid run-length sequence
                } else {
                    return false; // Invalid run-length pattern
                }
            } else {
                // Must be hex character for non-run-length parts
                let b = content[i];
                if !(b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b))
                {
                    return false;
                }
                i += 1;
            }
        }

        true
    }

    /// Check if content looks like thread info (comma-separated hex numbers)
    fn looks_like_thread_info(content: &[u8]) -> bool {
        if content.is_empty() {
            return false;
        }

        let content_str = match str::from_utf8(content) {
            Ok(s) => s,
            Err(_) => return false,
        };

        // Thread info should be comma-separated hex numbers or special values
        content_str
            .split(',')
            .all(|part| part == "0" || part == "-1" || part.chars().all(|c| c.is_ascii_hexdigit()))
    }

    /// Decode run-length encoded data from GDB
    /// Format: run of identical chars followed by '*' and repeat count (count+29)
    /// Example: "0* " -> "0000" (space = ASCII 32, so 32-29=3 more repeats)
    fn decode_run_length(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < data.len() {
            // Look for run-length encoding pattern: char + '*' + count
            if i + 2 < data.len() && data[i + 1] == b'*' {
                let repeated_char = data[i];
                let repeat_count_char = data[i + 2];

                // Decode the repeat count: n+29 encoding
                if repeat_count_char >= 29 {
                    let repeat_count = repeat_count_char - 29;

                    // Add the original character plus the repeated characters
                    // Total count = 1 (original) + repeat_count (additional)
                    for _ in 0..=repeat_count {
                        result.push(repeated_char);
                    }

                    i += 3; // Skip the char, '*', and count
                } else {
                    // Invalid encoding, treat as literal
                    result.push(data[i]);
                    i += 1;
                }
            } else {
                // No run-length encoding, copy as-is
                result.push(data[i]);
                i += 1;
            }
        }

        result
    }

    /// Decode hexadecimal data to bytes
    pub fn decode_hex(hex_data: &[u8]) -> Result<Vec<u8>, ParseError> {
        if hex_data.len() % 2 != 0 {
            return Err(ParseError::InvalidHex);
        }

        let hex_str = str::from_utf8(hex_data).map_err(|_| ParseError::InvalidHex)?;

        let mut result = Vec::new();
        for chunk in hex_str.as_bytes().chunks(2) {
            let hex_byte = str::from_utf8(chunk).map_err(|_| ParseError::InvalidHex)?;
            let byte = u8::from_str_radix(hex_byte, 16).map_err(|_| ParseError::InvalidHex)?;
            result.push(byte);
        }

        Ok(result)
    }

    /// Encode bytes as hexadecimal string
    pub fn encode_hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for GdbResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GdbResponse::Ack => write!(f, "ACK"),
            GdbResponse::Nack => write!(f, "NACK"),
            GdbResponse::Ok => write!(f, "OK"),
            GdbResponse::Empty => write!(f, "Empty"),
            GdbResponse::Error { code } => write!(f, "Error(0x{code:02x})"),
            GdbResponse::StopReply {
                signal,
                thread_id,
                reason,
            } => {
                write!(
                    f,
                    "Stop(signal=0x{signal:02x}, thread={thread_id:?}, reason={reason:?})"
                )
            }
            GdbResponse::MemoryData { data } => {
                write!(
                    f,
                    "Memory({} bytes: {})",
                    data.len(),
                    Self::encode_hex(&data[..data.len().min(8)])
                )
            }
            GdbResponse::RegisterData { data } => {
                write!(
                    f,
                    "Registers({} bytes: {})",
                    data.len(),
                    Self::encode_hex(&data[..data.len().min(8)])
                )
            }
            GdbResponse::ThreadInfo { threads, more_data } => {
                write!(f, "Threads({} threads, more={})", threads.len(), more_data)
            }
            GdbResponse::Supported { features } => {
                write!(f, "Supported({} features)", features.len())
            }
            GdbResponse::QXferData { data, is_final } => {
                let data_preview = String::from_utf8_lossy(&data[..data.len().min(32)]);
                write!(
                    f,
                    "QXfer({} bytes, final={}, preview: '{}')",
                    data.len(),
                    is_final,
                    data_preview
                )
            }
            GdbResponse::BinaryData { data } => {
                write!(f, "Binary({} bytes)", data.len())
            }
            GdbResponse::MonitorOutput { output } => {
                write!(f, "Monitor({})", output.trim())
            }
            GdbResponse::Raw { data } => {
                write!(
                    f,
                    "Raw({} bytes: {:?})",
                    data.len(),
                    String::from_utf8_lossy(&data[..data.len().min(16)])
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_logger() {
        INIT.call_once(|| {
            env_logger::Builder::from_default_env()
                .filter_level(log::LevelFilter::Info)
                .is_test(true)
                .init();
        });
    }

    #[test]
    fn test_parse_ack() {
        init_logger();
        assert_eq!(
            GdbResponse::parse(b"+", &Packet::default()).unwrap(),
            GdbResponse::Ack
        );
        assert_eq!(
            GdbResponse::parse(b"-", &Packet::default()).unwrap(),
            GdbResponse::Nack
        );
    }

    #[test]
    fn test_parse_empty() {
        init_logger();
        assert_eq!(
            GdbResponse::parse(b"$#00", &Packet::default()).unwrap(),
            GdbResponse::Empty
        );
    }

    #[test]
    fn test_parse_ok() {
        init_logger();
        assert_eq!(
            GdbResponse::parse(b"$OK#9a", &Packet::default()).unwrap(),
            GdbResponse::Ok
        );
    }

    #[test]
    fn test_parse_error() {
        init_logger();
        if let GdbResponse::Error { code } =
            GdbResponse::parse(b"$E01#a6", &Packet::default()).unwrap()
        {
            assert_eq!(code, 0x01);
        } else {
            panic!("Expected Error response");
        }
    }

    #[test]
    fn test_parse_hex_data() {
        init_logger();
        use crate::commands::{Base, GdbCommand};
        use crate::Packet;

        // Test with proper register read context
        let packet = Packet::Command(GdbCommand::Base(Base::LowerG));
        if let GdbResponse::RegisterData { data } =
            GdbResponse::parse(b"$deadbeef#20", &packet).unwrap()
        {
            assert_eq!(data, vec![0xde, 0xad, 0xbe, 0xef]);
        } else {
            panic!("Expected RegisterData response");
        }
    }

    #[test]
    fn test_invalid_checksum() {
        init_logger();
        match GdbResponse::parse(b"$OK#00", &Packet::default()) {
            Err(ParseError::InvalidChecksum) => {}
            _ => panic!("Expected checksum error"),
        }
    }

    #[test]
    fn test_run_length_decoding() {
        init_logger();
        // Test the example from the spec: "0* " -> "0000"
        // Space = ASCII 32, so 32-29=3 more repeats
        let input = b"0* ";
        let expected = b"0000";
        let result = GdbResponse::decode_run_length(input);
        assert_eq!(result, expected, "Should decode '0* ' to '0000'");

        // Test no run-length encoding
        let input = b"deadbeef";
        let expected = b"deadbeef";
        let result = GdbResponse::decode_run_length(input);
        assert_eq!(result, expected, "Should pass through normal hex unchanged");

        // Test multiple run-length sequences
        let input = b"a*!b*\""; // a*(33-29=4 more), b*(34-29=5 more)
        let expected = b"aaaaabbbbbb"; // 5 a's (1+4), 6 b's (1+5)
        let result = GdbResponse::decode_run_length(input);
        assert_eq!(
            result, expected,
            "Should handle multiple run-length sequences"
        );

        // Test edge case: minimum valid repeat count (29)
        let input = b"x*\x1d"; // 29 in decimal = \x1d, so 29-29=0 more repeats
        let expected = b"x";
        let result = GdbResponse::decode_run_length(input);
        assert_eq!(result, expected, "Should handle minimum repeat count");
    }

    #[test]
    fn test_is_hex_data_or_run_length() {
        init_logger();
        // Pure hex data
        assert!(GdbResponse::is_hex_data_or_run_length(b"deadbeef"));

        // Run-length encoded data
        assert!(GdbResponse::is_hex_data_or_run_length(b"0* "));

        // Mixed hex and run-length
        assert!(GdbResponse::is_hex_data_or_run_length(b"abc0* def"));

        // Should reject invalid characters
        assert!(!GdbResponse::is_hex_data_or_run_length(b"xyz"));
        assert!(!GdbResponse::is_hex_data_or_run_length(b""));
    }

    #[test]
    fn test_run_length_with_hex_parsing() {
        init_logger();
        // Test full integration: run-length decode then hex decode
        // "0* " should become "0000" then decode to bytes [0x00, 0x00]
        let input = b"0* ";
        let run_length_decoded = GdbResponse::decode_run_length(input);
        assert_eq!(run_length_decoded, b"0000");

        let hex_decoded = GdbResponse::decode_hex(&run_length_decoded).unwrap();
        assert_eq!(hex_decoded, vec![0x00, 0x00]);
    }
}
