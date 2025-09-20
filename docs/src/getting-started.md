# Getting Started

This guide will walk you through setting up DANG and running your first debugging session.

## Prerequisites

- Rust toolchain (1.76.0 or later)
- FST waveform files from RTL simulation
- Python 3.x (for signal mapping scripts)
- GDB or LLDB debugger

## Installation

See the [Installation](./installation.md) guide for detailed setup instructions.

## Your First Debug Session

### Step 1: Prepare Your Waveform Data

DANG requires:
1. **FST waveform file** - Generated from your RTL simulation
2. **Signal mapping script** - Python script defining how to extract CPU state from signals

Example directory structure:
```
test_data/ibex/
├── sim.fst              # Waveform file
└── signal_get.py        # Signal mapping
```

### Step 2: Start DANG

Launch DANG with your waveform data:

```bash
dang test_data/ibex/sim.fst --mapping-path test_data/ibex/signal_get.py
```

DANG will:
- Parse the waveform file
- Load the signal mapping
- Start a GDB server on port 9001
- Display connection information

### Step 3: Connect Your Debugger

#### Using LLDB
```bash
lldb -connect connect://localhost:9001
```

#### Using GDB
```bash
gdb -ex "target remote localhost:9001"
```

### Step 4: Debug Like Normal

Once connected, you can use standard GDB commands:

```gdb
# Set breakpoints
(gdb) break main
(gdb) break 0x1000

# Step through execution
(gdb) continue
(gdb) next
(gdb) step

# Examine memory and registers
(gdb) info registers
(gdb) x/10i $pc
(gdb) print variable_name
```

## Understanding the Output

DANG provides detailed logging about:
- Waveform parsing progress
- Signal mapping validation
- GDB protocol messages
- Execution state reconstruction

## Next Steps

- Learn about [Basic Usage](./basic-usage.md) patterns
- Explore [Examples](./examples.md) with different CPU designs
- Understand the [Architecture](./architecture/overview.md) for advanced use cases