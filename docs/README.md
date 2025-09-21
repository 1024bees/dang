# DANG Project Overview

DANG (**D**ebugger **A**nalysis **N**ext **G**eneration) is a debugging tool that brings GDB-style debugging capabilities to pre-silicon CPU development using waveform data from RTL simulations.

## Project Components

### DANG Core
The main application that coordinates waveform loading, signal mapping, and GDB server management. It acts as a bridge between RTL simulation waveforms and standard debugging tools.

### JPDB Module
Additional debugging utilities and extensions that provide:
- Custom debugging commands specific to waveform analysis
- Performance analysis tools for CPU simulation debugging
- Signal analysis and data export functionality
- Debug session management utilities

### Shucks Module
The GDB Remote Serial Protocol (RSP) implementation that enables standard debugger clients (GDB, LLDB) to communicate with DANG. It handles:
- Packet parsing and validation
- Command interpretation and routing
- Response formatting and transmission
- Protocol state management

## How It Works

1. **Waveform Loading**: DANG parses FST waveform files from RTL simulations
2. **Signal Mapping**: Python scripts define how to extract CPU state from simulation signals
3. **GDB Server**: Exposes a standard GDB server interface for familiar debugging workflows
4. **Debugger Connection**: Standard debuggers connect and debug the "execution" as if it were live

## Quick Start

```bash
# Start DANG with a waveform file and signal mapping
dang simulation.fst --mapping-path signals.py

# Connect with your preferred debugger
lldb -connect connect://localhost:9001
# or
gdb -ex "target remote localhost:9001"
```

This allows you to use familiar debugging commands like breakpoints, step execution, memory examination, and register inspection on pre-silicon CPU designs.