# Architecture Overview

DANG is built as a modular Rust workspace with three main components that work together to provide GDB debugging capabilities for pre-silicon CPUs.

## System Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   GDB Client    │────│  DANG Server    │────│  Waveform Data  │
│   (lldb/gdb)    │    │   (shucks)      │    │    (FST)        │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                              │
                       ┌─────────────┐
                       │Signal Mapping│
                       │  (Python)    │
                       └─────────────┘
```

## Workspace Components

### DANG Core (`dang/`)

The main application that orchestrates the entire debugging process:

- **CLI Interface**: Command-line argument parsing and configuration
- **Waveform Loading**: FST file parsing and signal extraction
- **Signal Mapping Integration**: Python script execution and signal resolution
- **Server Management**: GDB server lifecycle and connection handling
- **Runtime Coordination**: Managing the debugging session state

Key files:
- `cli.rs` - Command-line interface and argument parsing
- `gdb.rs` - GDB server implementation and protocol handling
- `waveloader.rs` - FST waveform file parsing and signal management
- `runtime.rs` - Debugging session state and execution control

### Shucks Module (`shucks/`)

The GDB protocol implementation that handles all debugger communication:

- **Protocol Implementation**: Complete GDB Remote Serial Protocol (RSP)
- **Packet Handling**: Parsing and generating GDB protocol packets
- **Command Processing**: Translating GDB commands to waveform operations
- **Response Generation**: Converting waveform data to GDB responses

Key files:
- `client.rs` - GDB client connection management and protocol state
- `packet.rs` - GDB packet parsing and serialization
- `commands.rs` - GDB command interpretation and routing
- `response.rs` - GDB response formatting and generation

### JPDB Module (`jpdb/`)

Additional debugging utilities and extensions:

- **Extended Commands**: Custom debugging commands beyond standard GDB
- **Utilities**: Helper functions for waveform analysis
- **Performance Tools**: Profiling and optimization utilities

## Data Flow

### 1. Initialization Phase

```
User Command → CLI Parser → Configuration → Waveform Loader
                                        ↓
Signal Mapping Script ← Python Interpreter ← FST Parser
```

1. User runs `dang` with waveform file and mapping script
2. CLI parser validates arguments and creates configuration
3. Waveform loader opens FST file and extracts signal hierarchy
4. Python interpreter loads mapping script and validates signal access
5. GDB server starts and waits for debugger connections

### 2. Connection Phase

```
GDB Client → TCP Connection → Shucks Protocol Handler → DANG Core
```

1. Debugger (GDB/LLDB) connects to DANG server
2. Shucks establishes protocol session and negotiates capabilities
3. Initial state synchronization between debugger and waveform data

### 3. Debugging Phase

```
GDB Command → Packet Parser → Command Router → Signal Mapper → Waveform Data
                                                      ↓
GDB Response ← Response Generator ← State Reconstructor ← Python Script
```

1. Debugger sends GDB protocol commands
2. Shucks parses packets and routes commands
3. DANG core translates commands to signal queries
4. Python mapping script extracts CPU state from waveform signals
5. Response generator formats results back to GDB protocol
6. Debugger receives and displays results

## Key Design Principles

### Modularity

Each component has clear responsibilities and interfaces:
- **DANG Core**: Application logic and coordination
- **Shucks**: Protocol handling and communication
- **JPDB**: Extensions and utilities

### Extensibility

The architecture supports multiple extension points:
- **Signal Mapping**: Python scripts for different CPU designs
- **Protocol Extensions**: Custom GDB commands through JPDB
- **Waveform Formats**: Pluggable parsers for different file formats

### Performance

Optimized for large waveform files:
- **Lazy Loading**: Signals loaded on-demand
- **Caching**: Frequently accessed data cached in memory
- **Streaming**: Large files processed incrementally

### Compatibility

Maintains compatibility with existing tools:
- **Standard GDB Protocol**: Works with any GDB-compatible debugger
- **Python Integration**: Leverages existing signal analysis scripts
- **File Format Support**: Uses industry-standard FST format

## State Management

### Waveform State

- **Time Cursor**: Current simulation time for state queries
- **Signal Cache**: Recently accessed signal values
- **Hierarchy Map**: Fast lookup of signal names to waveform data

### Debugging State

- **Breakpoints**: Mapped to specific simulation time points
- **Registers**: Extracted from waveform signals via Python mapping
- **Memory**: Reconstructed from memory-related signals
- **Stack**: Derived from register state and memory contents

### Protocol State

- **Connection State**: GDB client connection status
- **Feature Negotiation**: Agreed-upon protocol capabilities
- **Command History**: Recent commands for context and optimization

## Error Handling

### Graceful Degradation

- **Missing Signals**: Continue with available data
- **Mapping Errors**: Provide fallback behavior
- **Protocol Errors**: Maintain connection when possible

### Comprehensive Logging

- **Structured Logging**: Different levels for different components
- **Debug Information**: Detailed tracing for development
- **User Feedback**: Clear error messages and suggestions

## Security Considerations

### Input Validation

- **Waveform Files**: Validate FST file integrity
- **Python Scripts**: Sandbox execution environment
- **Network Protocol**: Validate all GDB protocol messages

### Resource Limits

- **Memory Usage**: Limits on cached waveform data
- **File Access**: Restricted to specified directories
- **Network Connections**: Rate limiting and timeout handling

## Next Steps

Explore the detailed architecture of each component:

- [DANG Core](./dang-core.md) - Main application architecture
- [Shucks Module](./shucks.md) - GDB protocol implementation
- [JPDB Module](./jpdb.md) - Debugging utilities and extensions
- [Waveform Loading](./waveform-loading.md) - FST parsing and signal management