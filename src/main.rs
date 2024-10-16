use std::{collections::HashMap, io::Write, mem::size_of, num::NonZeroUsize};

pub mod cli;
pub(crate) mod convert;
mod gdb;
pub mod runtime;
mod waveloader;

fn main() {
    println!("Hello, world!");
}
