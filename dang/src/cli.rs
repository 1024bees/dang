//! An incredibly simple emulator to run elf binaries compiled with
//! `arm-none-eabi-cc -march=armv4t`. It's not modeled after any real-world
//! system.

use crate::runtime;

use super::runtime::Waver;
use argh::FromArgs;
use gdbstub::common::Signal;
use gdbstub::conn::Connection;
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::run_blocking;
use gdbstub::stub::DisconnectReason;
use gdbstub::stub::GdbStub;
use gdbstub::stub::SingleThreadStopReason;
use gdbstub::target::Target;
use std::net::TcpStream;
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
    let sockaddr = format!("127.0.0.1:{port}");
    log::warn!("Waiting for a GDB connection on {sockaddr:?}...");

    let sock = TcpListener::bind(sockaddr)?;
    let actual_addr = sock.local_addr()?;
    log::warn!("Actually bound to {actual_addr:?}");

    let (stream, addr) = sock.accept()?;
    log::warn!("Debugger connected from {addr}");

    Ok(stream)
}

pub fn wait_for_tcp_with_port(port: u16) -> DynResult<(TcpStream, u16)> {
    let sockaddr = format!("127.0.0.1:{port}");
    log::debug!("Waiting for a GDB connection on {sockaddr:?}...");

    let sock = TcpListener::bind(sockaddr)?;
    let actual_addr = sock.local_addr()?;
    let actual_port = actual_addr.port();
    log::debug!("Actually bound to {actual_addr:?}");

    let (stream, addr) = sock.accept()?;
    log::debug!("Debugger connected from {addr}");

    Ok((stream, actual_port))
}

pub fn wait_for_tcp_with_listener(listener: TcpListener) -> DynResult<TcpStream> {
    let actual_addr = listener.local_addr()?;
    log::debug!("Waiting for a GDB connection on {actual_addr:?}...");

    let (stream, addr) = listener.accept()?;
    log::debug!("Debugger connected from {addr}");

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
    let DangArgs {
        wave_path,
        mapping_path,
        elf,
    } = argh::from_env();

    start_with_args(wave_path, mapping_path, elf)
}

pub fn start_with_args(wave_path: PathBuf, mapping_path: PathBuf, elf: PathBuf) -> DynResult<()> {
    start_with_args_and_port(wave_path, mapping_path, elf, 9001)
}

pub fn start_with_args_and_port(
    wave_path: PathBuf,
    mapping_path: PathBuf,
    elf: PathBuf,
    port: u16,
) -> DynResult<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();

    log::debug!("starting logger to stdout");

    let mut emu = Waver::new(wave_path, mapping_path, elf).expect("Could not create wave runtime");

    let connection: Box<dyn ConnectionExt<Error = std::io::Error>> =
        { Box::new(wait_for_tcp(port)?) };

    let gdb = GdbStub::new(connection);

    match gdb.run_blocking::<DangGdbEventLoop>(&mut emu) {
        Ok(disconnect_reason) => match disconnect_reason {
            DisconnectReason::Disconnect => {
                log::debug!("GDB client has disconnected. Running to completion...");
            }
            DisconnectReason::TargetExited(code) => {
                log::debug!("Target exited with code {code}!")
            }
            DisconnectReason::TargetTerminated(sig) => {
                log::debug!("Target terminated with signal {sig}!")
            }
            DisconnectReason::Kill => log::debug!("GDB sent a kill command!"),
        },
        Err(e) => {
            if e.is_target_error() {
                log::debug!(
                    "target encountered a fatal error: {}",
                    e.into_target_error().unwrap()
                )
            } else if e.is_connection_error() {
                let (e, kind) = e.into_connection_error().unwrap();
                log::debug!("connection error: {kind:?} - {e}",)
            } else {
                log::debug!("gdbstub encountered a fatal error: {e}")
            }
        }
    }

    log::debug!("Program completed");

    Ok(())
}

pub fn start_with_args_and_listener(
    wave_path: PathBuf,
    mapping_path: PathBuf,
    elf: PathBuf,
    listener: TcpListener,
) -> DynResult<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .try_init();
    log::debug!("started");

    let mut emu = Waver::new(wave_path, mapping_path, elf).expect("Could not create wave runtime");

    log::debug!("emulator made");

    let connection: Box<dyn ConnectionExt<Error = std::io::Error>> =
        { Box::new(wait_for_tcp_with_listener(listener)?) };

    log::debug!("connection made");

    let gdb = GdbStub::new(connection);

    log::debug!("gdb stub made");

    match gdb.run_blocking::<DangGdbEventLoop>(&mut emu) {
        Ok(disconnect_reason) => match disconnect_reason {
            DisconnectReason::Disconnect => {
                log::debug!("GDB client has disconnected. Running to completion...");
            }
            DisconnectReason::TargetExited(code) => {
                log::debug!("Target exited with code {code}!")
            }
            DisconnectReason::TargetTerminated(sig) => {
                log::debug!("Target terminated with signal {sig}!")
            }
            DisconnectReason::Kill => log::debug!("GDB sent a kill command!"),
        },
        Err(e) => {
            if e.is_target_error() {
                log::debug!(
                    "target encountered a fatal error: {}",
                    e.into_target_error().unwrap()
                )
            } else if e.is_connection_error() {
                let (e, kind) = e.into_connection_error().unwrap();
                log::debug!("connection error: {kind:?} - {e}",)
            } else {
                log::debug!("gdbstub encountered a fatal error: {e}")
            }
        }
    }

    log::debug!("Program completed");

    Ok(())
}

pub fn start_with_args_and_listener_silent(
    wave_path: PathBuf,
    mapping_path: PathBuf,
    elf: PathBuf,
    listener: TcpListener,
) -> DynResult<()> {
    // Initialize logger with error level only to suppress most output
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error"))
        .try_init();

    let mut emu = Waver::new(wave_path, mapping_path, elf).expect("Could not create wave runtime");

    let connection: Box<dyn ConnectionExt<Error = std::io::Error>> =
        { Box::new(wait_for_tcp_with_listener(listener)?) };

    let gdb = GdbStub::new(connection);

    match gdb.run_blocking::<DangGdbEventLoop>(&mut emu) {
        Ok(_disconnect_reason) => {
            // Suppressed all disconnect reason output
        }
        Err(_e) => {
            // Suppress all error output
        }
    }

    // Suppress "Program completed" message

    Ok(())
}
