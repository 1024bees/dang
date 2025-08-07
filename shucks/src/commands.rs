use std::io;

use crate::packet::{FinishedPacket, PacketCursor};

pub enum GdbCommand {
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
        cursor.write_content(self.base_str().as_bytes())?;

        match self {
            Self::QSupported => {
                cursor.write_content(b":xmlRegisters=riscv")?;
            }
            _ => {
                // pass
            }
        };
        cursor.finish()
    }
}