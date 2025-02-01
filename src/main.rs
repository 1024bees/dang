pub mod cli;
pub(crate) mod convert;
mod gdb;
pub mod runtime;
mod waveloader;

fn main() {
    let app_err = cli::start();
    if let Err(err) = app_err {
        panic!("Failed to run dang with error {}", err)
    }
}
