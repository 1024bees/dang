//! An incredibly simple emulator to run elf binaries compiled with
//! `arm-none-eabi-cc -march=armv4t`. It's not modeled after any real-world
//! system.

use crate::runtime;

use super::runtime::Waver;
use argh::FromArgs;
use gdbstub::conn::Connection;
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::run_blocking;
use gdbstub::stub::DisconnectReason;
use gdbstub::stub::GdbStub;
use gdbstub::stub::SingleThreadStopReason;
use gdbstub::target::Target;
use gdbstub::{common::Signal, target::ext::extended_mode::ExtendedMode};
use std::io::Write;
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::{net::TcpListener, path::PathBuf};

#[derive(FromArgs, Debug, Clone)]
/// CLI to dang Dang
struct DangArgs {
    #[argh(option)]
    /// path to the vcd, fst or ghw file that will be stepped through
    wave_path: PathBuf,

    #[argh(option)]
    /// path to a signal mapping file
    mapping_path: PathBuf,

    #[argh(option)]
    /// path to a signal mapping file
    elf: PathBuf,
}

type DynResult<T> = Result<T, Box<dyn std::error::Error>>;

fn wait_for_tcp(port: u16) -> DynResult<TcpStream> {
    let sockaddr = format!("127.0.0.1:{}", port);
    eprintln!("Waiting for a GDB connection on {:?}...", sockaddr);

    let sock = TcpListener::bind(sockaddr)?;
    let (stream, addr) = sock.accept()?;
    eprintln!("Debugger connected from {}", addr);

    Ok(stream)
}

#[cfg(unix)]
fn wait_for_uds(path: &str) -> DynResult<UnixStream> {
    match std::fs::remove_file(path) {
        Ok(_) => {}
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {}
            _ => return Err(e.into()),
        },
    }

    eprintln!("Waiting for a GDB connection on {}...", path);

    let sock = UnixListener::bind(path)?;
    let (stream, addr) = sock.accept()?;
    eprintln!("Debugger connected from {:?}", addr);

    Ok(stream)
}

enum DangGdbEventLoop {}

impl run_blocking::BlockingEventLoop for DangGdbEventLoop {
    type Target = Waver;
    type Connection = Box<dyn ConnectionExt<Error = std::io::Error>>;
    type StopReason = SingleThreadStopReason<u32>;

    #[allow(clippy::type_complexity)]
    fn wait_for_stop_reason(
        target: &mut Waver,
        conn: &mut Self::Connection,
    ) -> Result<
        run_blocking::Event<SingleThreadStopReason<u32>>,
        run_blocking::WaitForStopReasonError<
            <Self::Target as Target>::Error,
            <Self::Connection as Connection>::Error,
        >,
    > {
        let poll_incoming_data = || {
            // gdbstub takes ownership of the underlying connection, so the `borrow_conn`
            // method is used to borrow the underlying connection back from the stub to
            // check for incoming data.
            conn.peek().map(|b| b.is_some()).unwrap_or(true)
        };

        match target.run(poll_incoming_data) {
            runtime::RunEvent::IncomingData => {
                let byte = conn
                    .read()
                    .map_err(run_blocking::WaitForStopReasonError::Connection)?;
                Ok(run_blocking::Event::IncomingData(byte))
            }
            runtime::RunEvent::Event(event) => {
                // translate emulator stop reason into GDB stop reason

                let stop_reason = match event {
                    runtime::Event::DoneStep => SingleThreadStopReason::DoneStep,
                    runtime::Event::Halted => SingleThreadStopReason::Terminated(Signal::SIGSTOP),
                    runtime::Event::Break => SingleThreadStopReason::SwBreak(()),
                };

                Ok(run_blocking::Event::TargetStopped(stop_reason))
            }
        }
    }

    fn on_interrupt(
        _target: &mut Waver,
    ) -> Result<Option<SingleThreadStopReason<u32>>, <Waver as Target>::Error> {
        // Because this emulator runs as part of the GDB stub loop, there isn't any
        // special action that needs to be taken to interrupt the underlying target. It
        // is implicitly paused whenever the stub isn't within the
        // `wait_for_stop_reason` callback.
        Ok(Some(SingleThreadStopReason::Signal(Signal::SIGINT)))
    }
}

pub fn start() -> DynResult<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("starting logger to stdout");
    let DangArgs {
        wave_path,
        mapping_path,
        elf,
    } = argh::from_env();

    let mut emu = Waver::new(wave_path, mapping_path, elf).expect("Could not create wave runtime");

    let connection: Box<dyn ConnectionExt<Error = std::io::Error>> =
        { Box::new(wait_for_tcp(9001)?) };

    let gdb = GdbStub::new(connection);

    match gdb.run_blocking::<DangGdbEventLoop>(&mut emu) {
        Ok(disconnect_reason) => match disconnect_reason {
            DisconnectReason::Disconnect => {
                println!("GDB client has disconnected. Running to completion...");
            }
            DisconnectReason::TargetExited(code) => {
                println!("Target exited with code {}!", code)
            }
            DisconnectReason::TargetTerminated(sig) => {
                println!("Target terminated with signal {}!", sig)
            }
            DisconnectReason::Kill => println!("GDB sent a kill command!"),
        },
        Err(e) => {
            if e.is_target_error() {
                println!(
                    "target encountered a fatal error: {}",
                    e.into_target_error().unwrap()
                )
            } else if e.is_connection_error() {
                let (e, kind) = e.into_connection_error().unwrap();
                println!("connection error: {:?} - {}", kind, e,)
            } else {
                println!("gdbstub encountered a fatal error: {}", e)
            }
        }
    }

    println!("Program completed");

    Ok(())
}
