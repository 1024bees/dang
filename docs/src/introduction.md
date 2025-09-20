# Introduction

DANG is a revolutionary debugging tool that brings GDB-style debugging capabilities to pre-silicon CPU development. Unlike traditional debuggers that work with running processes, DANG uses waveform data from RTL simulations to recreate program executions and expose them through a standard GDB server interface.

## What is DANG?

DANG stands for **D**ebugger **A**nalysis **N**ext **G**eneration. It bridges the gap between hardware simulation and software debugging by:

- **Parsing waveform files** (FST format) from RTL simulations
- **Reconstructing program execution** state from signal traces
- **Exposing a GDB server interface** for familiar debugging workflows
- **Supporting standard debuggers** like GDB and LLDB

## Why DANG?

Pre-silicon CPU development traditionally involves:
- Running RTL simulations to verify CPU behavior
- Manually analyzing waveforms to understand program execution
- Switching between different tools for hardware and software analysis

DANG eliminates this friction by providing a unified debugging interface that software developers already know and love.

## Key Features

- **Standard GDB Protocol**: Connect with any GDB-compatible debugger
- **Waveform-Based Analysis**: No need for intrusive debug hardware
- **Cross-Platform Support**: Works on macOS, Linux, and Windows
- **Signal Mapping**: Flexible configuration for different CPU designs
- **Workspace Architecture**: Modular design with specialized components

## Quick Start

```bash
# Start DANG with a waveform file
dang test_data/ibex/sim.fst --mapping-path test_data/ibex/signal_get.py

# Connect with LLDB
lldb -connect connect://localhost:9001

# Or connect with GDB
gdb -ex "target remote localhost:9001"
```

Ready to dive deeper? Continue to the [Getting Started](./getting-started.md) guide.