use std::{
    io::{self, Cursor, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    ops::Add,
};

/// Top-Level GDB packet
pub enum Packet {
    Ack,
    //Nack,
    //Interrupt,
    Command(Command),
}

enum Command {
    Base(Base),
    Resume(Resume),
}

pub enum Base {
    QuestionMark,
    D,
    LowerG,
    UpperG,
    H,
    K,
    LowerM,
    UpperM,
    QAttached,
    QfThreadInfo,
    QsThreadInfo,
    QSupported,
    T,
    VKill,
    QStartNoAckMode,
}

pub enum Resume {
    Continue,
    Step,
    VCont,
}

pub struct PacketCursor<'a> {
    cursor: Cursor<&'a mut [u8]>,
    sum: u64,
}

pub struct FinishedPacket<'a>(&'a [u8]);

impl<'a> PacketCursor<'a> {
    pub fn new(slice: &'a mut [u8]) -> Self {
        Self {
            cursor: Cursor::new(slice),
            sum: 0,
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        let sum = buf.iter().fold(0u64, |a, b| a.add(*b as u64));
        self.sum += sum;
        self.cursor.write(buf)
    }
    pub fn finish(mut self) -> Result<FinishedPacket<'a>, std::io::Error> {
        let modsum = self.sum % 256;
        let str = format!("#{modsum:x}");
        self.cursor.write(str.as_bytes())?;
        let slice_end = self.cursor.position() as usize;
        let slice = &self.cursor.into_inner()[0..slice_end];
        Ok(FinishedPacket(slice))
    }
}

impl Packet {
    fn ack() -> Result<FinishedPacket<'static>, io::Error> {
        Ok(FinishedPacket("+".as_bytes()))
    }

    fn to_finished_packet<'a>(&self, slice: &'a mut [u8]) -> Result<FinishedPacket<'a>, io::Error> {
        let rv = match self {
            Self::Ack => Packet::ack(),
            Self::Command(command) => command.to_command(slice),
        };
        rv
    }
}

impl Command {
    fn to_command<'a>(&self, slice: &'a mut [u8]) -> Result<FinishedPacket<'a>, io::Error> {
        match self {
            Self::Base(base) => base.to_cmd(slice),
            Self::Resume(resume) => resume.to_cmd(slice),
        }
    }
}

impl Resume {
    fn base_str(&self) -> &'static str {
        match self {
            Self::Step => "s",
            Self::Continue => "c",
            Self::VCont => "vCont",
        }
    }

    pub fn to_cmd<'a>(&self, slice: &'a mut [u8]) -> Result<FinishedPacket<'a>, io::Error> {
        let mut cursor = PacketCursor::new(slice);
        cursor.write(b"$")?;
        cursor.write(self.base_str().as_bytes())?;

        match self {
            _ => {
                //pass
            }
        };
        cursor.finish()
    }
}

impl Base {
    fn base_str(&self) -> &'static str {
        match self {
            Self::QuestionMark => "?",
            Self::D => "D",
            Self::LowerG => "g",
            Self::UpperG => "G",
            Self::H => "H",
            Self::K => "k",
            Self::LowerM => "m",
            Self::UpperM => "M",
            Self::QsThreadInfo => "qsThreadInfo",
            Self::QfThreadInfo => "qfThreadInfo",
            Self::QSupported => "qSupported",
            Self::VKill => "vKill",
            Self::QStartNoAckMode => "QStartNoAckMode",
            Self::QAttached => "qAttached",
            Self::T => "T",
        }
    }

    pub fn to_cmd<'a>(&self, slice: &'a mut [u8]) -> Result<FinishedPacket<'a>, io::Error> {
        let mut cursor = PacketCursor::new(slice);
        cursor.write(b"$")?;
        cursor.write(self.base_str().as_bytes())?;

        match self {
            _ => {
                //pass
            }
        };
        cursor.finish()
    }
}

pub struct Client {
    strm: TcpStream,
}

impl Client {
    pub fn new() -> Self {
        let addr = "127.0.0.1:9001";
        let strm = TcpStream::connect(addr).unwrap();
        Self { strm }
    }
}

fn main() {
    println!("Hello, world!");
}
