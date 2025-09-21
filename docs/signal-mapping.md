# Signal Mapping File Semantics

Signal mapping files are Python scripts that define how DANG extracts CPU state from waveform signals. These scripts serve as the bridge between RTL simulation signals and the debugging interface.

## Purpose

The mapping file translates hierarchical signal names from your RTL design into the register and memory state that debuggers expect. This allows DANG to present a familiar debugging interface regardless of the underlying CPU design.

## Required Functions

Your mapping script must implement specific functions that DANG calls during execution:

### Core Functions

```python
def get_gdb_signals(wave: Waveform) -> dict:
    """
    Extract the main signals needed for GDB debugging.
    Must return a dictionary with 'pc' and register signals.

    Args:
        wave: PyWellen Waveform object

    Returns:
        Dictionary mapping signal names to Signal objects
    """
```

### Optional Functions

```python
def get_misc_signals(wave: Waveform) -> List[Signal]:
    """
    Extract additional signals for analysis.

    Args:
        wave: PyWellen Waveform object

    Returns:
        List of additional Signal objects
    """
```

## Signal Mapping Semantics

### PyWellen Integration

DANG uses the PyWellen library to access waveform data. Your mapping script works with PyWellen's Waveform and Signal objects:

```python
from pywellen import Waveform, Signal
from typing import List, Dict

def get_gdb_signals(wave: Waveform) -> Dict[str, Signal]:
    # Extract program counter
    pc = wave.get_signal_from_path(
        "TOP.ibex_simple_system.u_top.u_ibex_top.u_ibex_core.wb_stage_i.pc_wb_o"
    )

    # Extract general-purpose registers
    gprs = {
        f"x{i}": wave.get_signal_from_path(
            f"TOP.ibex_simple_system.u_top.u_ibex_top.gen_regfile_ff.register_file_i.rf_reg.[{i}]"
        ).sliced(0, 31)  # Extract bits 0-31
        for i in range(32)
    }

    return {"pc": pc, **gprs}
```

### Signal Path Conventions

Signal paths follow the hierarchical structure of your RTL design:

- **Full paths**: Start from the top-level module (e.g., `TOP.module.submodule.signal`)
- **Array indexing**: Use bracket notation for arrays (e.g., `rf_reg.[0]`, `rf_reg.[15]`)
- **Bit slicing**: Use `.sliced(start, end)` to extract specific bits

### Standard Signal Names

The returned dictionary should use standard register naming conventions:

```python
# RISC-V register naming
return {
    "pc": pc_signal,           # Program counter
    "x0": zero_reg,            # Zero register
    "x1": ra_reg,              # Return address
    "x2": sp_reg,              # Stack pointer
    # ... up to x31
}
```

## Example: RISC-V Ibex Core

Here's a complete example for the Ibex RISC-V core:

```python
from pywellen import Waveform, Signal
from typing import List, Dict

def get_gdb_signals(wave: Waveform) -> Dict[str, Signal]:
    """Extract signals for RISC-V Ibex core debugging."""

    # Program counter from writeback stage
    pc = wave.get_signal_from_path(
        "TOP.ibex_simple_system.u_top.u_ibex_top.u_ibex_core.wb_stage_i.pc_wb_o"
    )

    # General-purpose registers (x0-x31)
    gprs = {}
    for i in range(32):
        reg_signal = wave.get_signal_from_path(
            f"TOP.ibex_simple_system.u_top.u_ibex_top.gen_regfile_ff.register_file_i.rf_reg.[{i}]"
        )
        # Extract 32-bit register value (bits 0-31)
        gprs[f"x{i}"] = reg_signal.sliced(0, 31)

    return {"pc": pc, **gprs}

def get_misc_signals(wave: Waveform) -> List[Signal]:
    """Extract additional signals for analysis."""
    return [
        wave.get_signal_from_path(
            "TOP.ibex_simple_system.u_top.u_ibex_top.u_ibex_core.wb_stage_i.pc_wb_o"
        )
    ]
```

## Signal Naming Guidelines

### Hierarchy Navigation

- Start from the testbench top module (often `TOP`)
- Follow the instantiation hierarchy downward
- Use the exact module instance names from your RTL

### Common Signal Locations

Different CPU designs store key signals in different locations:

```python
# Examples for different stages
pc_if = "cpu.if_stage.pc"           # Instruction fetch
pc_id = "cpu.id_stage.pc"           # Instruction decode
pc_ex = "cpu.ex_stage.pc"           # Execute
pc_wb = "cpu.wb_stage.pc"           # Writeback (most stable)

# Register file locations
regfile_ff = "cpu.regfile_ff.regs"        # Flip-flop based
regfile_latch = "cpu.regfile_latch.regs"  # Latch based
```

### Signal Stability

Choose signals from stable pipeline stages:

- **Writeback stage**: Most stable, reflects committed state
- **Execute stage**: May change due to pipeline flushes
- **Fetch/Decode**: Least stable, speculative execution

## Bit Manipulation

### Extracting Bit Ranges

```python
# Extract specific bit ranges
signal_32bit = wave_signal.sliced(0, 31)    # Bits 0-31 (32 bits)
signal_16bit = wave_signal.sliced(0, 15)    # Bits 0-15 (16 bits)
signal_8bit = wave_signal.sliced(0, 7)      # Bits 0-7 (8 bits)

# Extract single bit
enable_bit = control_signal.sliced(0, 0)    # Bit 0 only
```

### Handling Different Data Widths

```python
# Handle different register widths
if cpu_width == 64:
    reg_signal = wave_signal.sliced(0, 63)   # 64-bit
elif cpu_width == 32:
    reg_signal = wave_signal.sliced(0, 31)   # 32-bit
else:
    reg_signal = wave_signal.sliced(0, 15)   # 16-bit
```

## Debugging Mapping Issues

### Common Problems

1. **Signal not found**: Check signal path spelling and hierarchy
2. **Wrong bit width**: Verify bit slicing parameters
3. **Timing issues**: Ensure signals are from stable pipeline stages
4. **Array indexing**: Use correct bracket notation for arrays

### Testing Your Mapping

```python
# Add debug prints to your mapping function
def get_gdb_signals(wave: Waveform) -> Dict[str, Signal]:
    try:
        pc = wave.get_signal_from_path("TOP.cpu.pc")
        print(f"Found PC signal: {pc}")
    except Exception as e:
        print(f"PC signal not found: {e}")

    return {"pc": pc, ...}
```

## Best Practices

1. **Use writeback stage signals** for stability
2. **Add error handling** for missing signals
3. **Document your signal paths** with comments
4. **Test with small waveforms** first
5. **Verify register values** match expected behavior
6. **Use meaningful variable names** in your mapping code