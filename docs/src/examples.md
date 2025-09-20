# Examples

This section provides practical examples of using DANG with different CPU designs and debugging scenarios.

## RISC-V Ibex Core

The included example demonstrates DANG with the open-source Ibex RISC-V core.

### Files

```
test_data/ibex/
├── sim.fst              # Waveform from Ibex simulation
└── signal_get.py        # Signal mapping for Ibex
```

### Running the Example

```bash
# Start DANG with Ibex data
dang test_data/ibex/sim.fst --mapping-path test_data/ibex/signal_get.py

# Connect with LLDB
lldb -connect connect://localhost:9001
```

### Ibex Signal Mapping

The `signal_get.py` script maps Ibex's specific signal hierarchy:

```python
def get_pc(signals, time):
    """Extract program counter from Ibex IF stage"""
    return signals['ibex_core.if_stage_i.pc_id_o'].value_at(time)

def get_instruction(signals, time):
    """Get current instruction from ID stage"""
    return signals['ibex_core.id_stage_i.instr_rdata_i'].value_at(time)

def get_registers(signals, time):
    """Extract register file state"""
    regs = {}
    for i in range(32):
        signal_name = f'ibex_core.id_stage_i.register_file_i.rf_reg_q[{i}]'
        regs[f'x{i}'] = signals[signal_name].value_at(time)
    return regs
```

### Debugging Session

```gdb
# Connect and examine initial state
(lldb) register read pc
(lldb) memory read --format instruction --count 5 `$pc

# Set breakpoint at main function
(lldb) breakpoint set --address 0x100

# Step through boot sequence
(lldb) continue
(lldb) thread step-inst
(lldb) register read --all
```

## Custom CPU Example

Here's how to adapt DANG for your own CPU design.

### Step 1: Create Signal Mapping

Create a mapping script for your CPU's signal hierarchy:

```python
# custom_cpu_mapping.py

# Define your CPU's signal paths
PC_SIGNAL = 'top.cpu.core.pc'
INSTRUCTION_SIGNAL = 'top.cpu.core.instruction'
REGFILE_BASE = 'top.cpu.core.regfile.regs'

def get_pc(signals, time):
    """Extract program counter"""
    return signals[PC_SIGNAL].value_at(time)

def get_instruction(signals, time):
    """Get current instruction"""
    return signals[INSTRUCTION_SIGNAL].value_at(time)

def get_registers(signals, time):
    """Extract all registers"""
    regs = {}
    # Adapt to your register file structure
    for i in range(16):  # Example: 16 registers
        signal = f'{REGFILE_BASE}[{i}]'
        regs[f'r{i}'] = signals[signal].value_at(time)
    return regs

def get_memory(signals, time, address, size):
    """Extract memory contents"""
    # Implement based on your memory hierarchy
    # This might involve multiple signals for different memory regions
    if 0x0000 <= address < 0x1000:
        # ROM region
        return read_rom_signals(signals, time, address, size)
    elif 0x2000 <= address < 0x3000:
        # RAM region
        return read_ram_signals(signals, time, address, size)
    else:
        return [0] * size  # Unmapped region

def read_rom_signals(signals, time, address, size):
    """Read from ROM signals"""
    data = []
    for offset in range(size):
        addr = address + offset
        signal_name = f'top.memory.rom.mem[{addr}]'
        if signal_name in signals:
            data.append(signals[signal_name].value_at(time))
        else:
            data.append(0)
    return data
```

### Step 2: Generate Waveform

Run your RTL simulation to generate an FST file:

```bash
# Example with Verilator
verilator --trace-fst --exe --build sim.cpp top.v
./obj_dir/Vtop
# Produces trace.fst
```

### Step 3: Debug Session

```bash
# Start DANG with your files
dang trace.fst --mapping-path custom_cpu_mapping.py

# Connect and debug
gdb -ex "target remote localhost:9001"
```

## Advanced Debugging Scenarios

### Boot Sequence Analysis

```python
# Add to your mapping script
def get_boot_state(signals, time):
    """Check boot/reset state"""
    reset_signal = 'top.cpu.reset'
    boot_rom_en = 'top.memory.boot_rom_enable'

    return {
        'in_reset': signals[reset_signal].value_at(time),
        'boot_mode': signals[boot_rom_en].value_at(time)
    }
```

```gdb
# Debug boot sequence
(gdb) break *0x0        # Reset vector
(gdb) continue
(gdb) info registers
(gdb) x/20i $pc        # Show boot code
```

### Cache Analysis

```python
def get_cache_stats(signals, time):
    """Extract cache performance data"""
    return {
        'i_cache_hits': signals['top.cpu.icache.hits'].value_at(time),
        'i_cache_misses': signals['top.cpu.icache.misses'].value_at(time),
        'd_cache_hits': signals['top.cpu.dcache.hits'].value_at(time),
        'd_cache_misses': signals['top.cpu.dcache.misses'].value_at(time)
    }
```

### Pipeline State Monitoring

```python
def get_pipeline_state(signals, time):
    """Monitor pipeline stages"""
    return {
        'fetch_valid': signals['top.cpu.fetch.valid'].value_at(time),
        'decode_valid': signals['top.cpu.decode.valid'].value_at(time),
        'execute_valid': signals['top.cpu.execute.valid'].value_at(time),
        'writeback_valid': signals['top.cpu.writeback.valid'].value_at(time),
        'pipeline_stall': signals['top.cpu.stall'].value_at(time)
    }
```

## Performance Profiling

### Instruction Trace

```gdb
# Log all executed instructions
(gdb) while 1
>   printf "PC: 0x%x INSTR: 0x%x\n", $pc, *(uint32_t*)$pc
>   stepi
> end
```

### Function Call Tracing

```gdb
# Set up function entry/exit tracing
(gdb) break function_start
(gdb) commands
> silent
> printf "ENTER: %s\n", $function_name
> continue
> end

(gdb) break function_end
(gdb) commands
> silent
> printf "EXIT: %s\n", $function_name
> continue
> end
```

### Memory Access Patterns

```python
# Add to mapping script for memory debugging
def get_memory_transactions(signals, time):
    """Monitor memory bus activity"""
    return {
        'mem_valid': signals['top.memory_bus.valid'].value_at(time),
        'mem_ready': signals['top.memory_bus.ready'].value_at(time),
        'mem_addr': signals['top.memory_bus.addr'].value_at(time),
        'mem_wdata': signals['top.memory_bus.wdata'].value_at(time),
        'mem_rdata': signals['top.memory_bus.rdata'].value_at(time),
        'mem_we': signals['top.memory_bus.we'].value_at(time)
    }
```

## Tips and Best Practices

### Signal Mapping Tips

1. **Start simple**: Begin with PC and basic registers
2. **Validate signals**: Check signal names match your RTL hierarchy
3. **Handle timing**: Consider clock domains and signal delays
4. **Add error handling**: Gracefully handle missing signals

### Debugging Tips

1. **Use watchpoints**: Monitor specific memory locations
2. **Script repetitive tasks**: Automate common debugging patterns
3. **Save sessions**: Use GDB's logging features
4. **Compare simulations**: Debug multiple waveforms side-by-side

### Performance Tips

1. **Optimize signal access**: Cache frequently accessed signals
2. **Limit trace scope**: Focus on relevant time ranges
3. **Use efficient data types**: Choose appropriate precision for signals
4. **Parallel debugging**: Run multiple DANG instances on different ports

## Next Steps

- Learn about [Advanced Topics](./advanced/gdb-protocol.md)
- Understand the [Architecture](./architecture/overview.md)
- Explore [Signal Mapping](./advanced/signal-mapping.md) in detail