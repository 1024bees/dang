# Basic Usage

This guide covers the fundamental usage patterns and command-line options for DANG.

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
# Basic usage
dang simulation.fst --mapping-path signals.py

# Custom port
dang simulation.fst --mapping-path signals.py --port 8080

# Verbose logging
dang simulation.fst --mapping-path signals.py --log-level debug
```

## Signal Mapping

Signal mapping scripts define how DANG extracts CPU state from waveform signals. These Python scripts must implement specific functions that DANG calls during execution.

### Basic Mapping Script

```python
def get_pc(signals, time):
    """Extract program counter at given time"""
    return signals['cpu.pc'].value_at(time)

def get_registers(signals, time):
    """Extract register file state"""
    regs = {}
    for i in range(32):
        regs[f'x{i}'] = signals[f'cpu.regfile.regs[{i}]'].value_at(time)
    return regs

def get_memory(signals, time, address, size):
    """Extract memory contents"""
    # Implementation depends on your memory hierarchy
    pass
```

### Signal Naming Conventions

The mapping script should handle your specific CPU's signal names:

```python
# Example for RISC-V Ibex core
SIGNAL_MAP = {
    'pc': 'ibex_core.if_stage_i.pc_id_o',
    'instruction': 'ibex_core.id_stage_i.instr_rdata_i',
    'regfile_base': 'ibex_core.id_stage_i.register_file_i.rf_reg_q'
}
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

### 3. Set Breakpoints

```gdb
# Break at address
(gdb) break *0x1000

# Break at function (requires symbols)
(gdb) break main

# Break at instruction count
(gdb) break +100
```

### 4. Control Execution

```gdb
# Continue execution
(gdb) continue

# Step single instruction
(gdb) stepi

# Step over function calls
(gdb) nexti

# Run until return
(gdb) finish
```

### 5. Examine State

```gdb
# Show registers
(gdb) info registers

# Examine memory
(gdb) x/10i $pc          # 10 instructions at PC
(gdb) x/16x 0x2000       # 16 bytes hex at 0x2000

# Show stack trace
(gdb) backtrace
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
- Increase system **RAM** for large files
- Consider **file compression** (FST already compressed)
- Use **SSD storage** for faster file I/O

### Signal Mapping Optimization

- **Cache expensive calculations** in mapping scripts
- **Minimize signal lookups** per time step
- **Use efficient data structures** for signal access

### GDB Protocol Optimization

- **Batch register reads** when possible
- **Limit memory examination** scope
- **Use hardware breakpoints** (mapped to signal conditions)

## Common Patterns

### Debugging Boot Sequence

```gdb
(gdb) break *0x0           # Reset vector
(gdb) continue
(gdb) x/10i $pc           # Show boot code
```

### Tracing Function Calls

```gdb
(gdb) break function_entry
(gdb) commands
> silent
> printf "Entering function at %p\n", $pc
> continue
> end
```

### Memory Access Patterns

```gdb
(gdb) watch *0x2000       # Watch memory location
(gdb) continue            # Continue until access
```

## Next Steps

- Explore [Examples](./examples.md) with real CPU designs
- Learn about [Advanced Topics](./advanced/gdb-protocol.md)
- Understand the [Architecture](./architecture/overview.md)