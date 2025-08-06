use std::{
    io::{Cursor, Write},
    ops::Add,
};

pub struct PacketCursor<'a> {
    cursor: Cursor<&'a mut [u8]>,
    sum: u64,
}

pub struct FinishedPacket<'a>(pub &'a [u8]);

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