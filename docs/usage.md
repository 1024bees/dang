# JPDB and DANG Usage Guide

This guide covers the fundamental usage patterns and command-line options for DANG with JPDB utilities.

## Command Line Interface

### Basic Syntax

```bash
dang [OPTIONS] <waveform-file>
```

### Required Arguments

- **`<waveform-file>`**: Path to the FST waveform file from RTL simulation

### Common Options

- **`--mapping-path <path>`**: Path to Python signal mapping script
- **`--port <port>`**: GDB server port (default: 9001)
- **`--log-level <level>`**: Logging verbosity (error, warn, info, debug, trace)
- **`--help`**: Show help information
- **`--version`**: Show version information

### Examples

```bash
# Basic usage with JPDB utilities
dang simulation.fst --mapping-path signals.py

# Custom port
dang simulation.fst --mapping-path signals.py --port 8080

# Verbose logging for debugging
dang simulation.fst --mapping-path signals.py --log-level debug
```

## Debugging Workflow

### 1. Start DANG Server

```bash
dang waveform.fst --mapping-path mapping.py
```

Output should show:
```
[INFO] Loading waveform: waveform.fst
[INFO] Parsing signal mapping: mapping.py
[INFO] GDB server listening on port 9001
[INFO] Waiting for debugger connection...
```

### 2. Connect Debugger

In a separate terminal:

```bash
# Using LLDB
lldb -connect connect://localhost:9001

# Using GDB
gdb -ex "target remote localhost:9001"
```

### 3. Use Standard GDB Commands

```gdb
# Set breakpoints
(gdb) break *0x1000
(gdb) break main

# Control execution
(gdb) continue
(gdb) stepi
(gdb) nexti

# Examine state
(gdb) info registers
(gdb) x/10i $pc
(gdb) x/16x 0x2000
(gdb) backtrace
```

## JPDB Utilities

### Performance Analysis

JPDB provides additional debugging utilities beyond the core DANG functionality:

- **Signal trace commands**: Analyze signal behavior over time
- **Performance profiling**: Monitor query performance and cache hit rates
- **Memory usage tracking**: Track memory usage during debugging sessions

### Custom Commands

Extended GDB commands specific to waveform debugging:

```gdb
# Time navigation (JPDB-specific)
(jpdb) time-step 1000      # Jump to specific simulation time
(jpdb) time-range          # Show current time range
(jpdb) signal-trace pc     # Trace PC signal over time
```

## Configuration

### Environment Variables

- **`RUST_LOG`**: Override log level (e.g., `RUST_LOG=debug`)
- **`DANG_PORT`**: Default server port
- **`DANG_MAPPING_PATH`**: Default signal mapping script

### Log Levels

- **`error`**: Only errors and critical issues
- **`warn`**: Warnings and errors
- **`info`**: General information (default)
- **`debug`**: Detailed debugging information
- **`trace`**: Very verbose tracing

Example with environment variable:
```bash
RUST_LOG=debug dang simulation.fst --mapping-path signals.py
```

## Performance Tips

### Large Waveform Files

- Use **release builds** for better performance
- Increase system **RAM** for large files (16GB+ recommended)
- Use **SSD storage** for faster file I/O
- Consider **signal caching** in JPDB for frequently accessed signals

### Signal Mapping Optimization

- **Cache expensive calculations** in mapping scripts
- **Minimize signal lookups** per time step
- **Use efficient data structures** for signal access
- **Leverage JPDB's caching mechanisms**

### JPDB-Specific Optimizations

- **Batch operations**: Group multiple signal queries together
- **Query optimization**: Use JPDB's performance monitoring to identify bottlenecks
- **Cache management**: Configure cache sizes based on available memory

## Common Debugging Patterns

### Debugging Boot Sequence

```gdb
(gdb) break *0x0           # Reset vector
(gdb) continue
(gdb) x/10i $pc           # Show boot code
```

### Waveform Time Navigation

```bash
# Use JPDB time navigation features
(jpdb) time-step 5000      # Jump to simulation time 5000
(jpdb) signal-value pc     # Get PC value at current time
```

### Performance Analysis

```bash
# Monitor JPDB performance
(jpdb) cache-stats         # Show cache hit/miss statistics
(jpdb) query-time          # Show recent query performance
```