use std::io;

use crate::packet::{FinishedPacket, PacketCursor};

#[derive(Clone)]
pub enum GdbCommand {
    Base(Base),
    Resume(Resume),
}
#[derive(Clone)]
pub enum Base {
    QuestionMark,
    D,
    LowerG,
    UpperG,
    H,
    K,
    LowerM { addr: u32, length: u32 },
    UpperM,
    QAttached,
    QfThreadInfo,
    QsThreadInfo,
    QSupported,
    T,
    VKill,
    QStartNoAckMode,
    QXferExecFile { offset: u32, length: u32 },
}

#[derive(Clone)]
pub enum Resume {
    Continue,
    Step,
    VCont,
}

impl GdbCommand {
    pub fn to_command<'a>(&self, slice: &'a mut [u8]) -> Result<FinishedPacket<'a>, io::Error> {
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
        cursor.write_content(self.base_str().as_bytes())?;

        {
            //pass
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
            Self::LowerM { .. } => "m",
            Self::UpperM => "M",
            Self::QsThreadInfo => "qsThreadInfo",
            Self::QfThreadInfo => "qfThreadInfo",
            Self::QSupported => "qSupported",
            Self::VKill => "vKill",
            Self::QStartNoAckMode => "QStartNoAckMode",
            Self::QAttached => "qAttached",
            Self::T => "T",
            Self::QXferExecFile { .. } => "qXfer:exec-file:read",
        }
    }

    pub fn to_cmd<'a>(&self, slice: &'a mut [u8]) -> Result<FinishedPacket<'a>, io::Error> {
        let mut cursor = PacketCursor::new(slice);
        cursor.write(b"$")?;
        cursor.write_content(self.base_str().as_bytes())?;

        match self {
            Self::QSupported => {
                cursor.write_content(b":xmlRegisters=riscv")?;
            }
            Self::LowerM { addr, length } => {
                cursor.write_content(format!("{:x},{:x}", addr, length).as_bytes())?;
            }
            Self::QXferExecFile { offset, length } => {
                cursor.write_content(format!("::{:x},{:x}", offset, length).as_bytes())?;
            }
            _ => {
                // pass
            }
        };
        cursor.finish()
    }
}

