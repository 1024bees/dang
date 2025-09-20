# DANG Core Architecture

The DANG core (`dang/`) is the main application that coordinates waveform loading, signal mapping, and GDB server management.

## Module Structure

```
dang/src/
├── main.rs          # Application entry point
├── lib.rs           # Library interface
├── cli.rs           # Command-line interface
├── gdb.rs           # GDB server implementation
├── waveloader.rs    # Waveform file parsing
├── runtime.rs       # Debugging session management
└── convert.rs       # Data type conversions
```

## Component Details

### CLI Interface (`cli.rs`)

Handles command-line argument parsing and configuration:

```rust
pub struct Config {
    pub waveform_path: PathBuf,
    pub mapping_path: Option<PathBuf>,
    pub port: u16,
    pub log_level: LevelFilter,
}
```

Key responsibilities:
- Parse command-line arguments using `clap`
- Validate file paths and options
- Set up logging configuration
- Create application configuration struct

### GDB Server (`gdb.rs`)

Implements the GDB server that communicates with debugger clients:

```rust
pub struct GdbServer {
    listener: TcpListener,
    waveform: WaveformData,
    mapping: SignalMapping,
}
```

Key responsibilities:
- TCP server management
- Client connection handling
- Integration with Shucks protocol handler
- Session state management

### Waveform Loader (`waveloader.rs`)

Handles FST file parsing and signal data extraction:

```rust
pub struct WaveformData {
    signals: HashMap<String, SignalHandle>,
    time_range: (u64, u64),
    current_time: u64,
}
```

Key responsibilities:
- FST file format parsing
- Signal hierarchy extraction
- Time-based signal value queries
- Memory-efficient signal caching

### Runtime Management (`runtime.rs`)

Manages the debugging session state and execution flow:

```rust
pub struct DebugRuntime {
    breakpoints: Vec<Breakpoint>,
    execution_state: ExecutionState,
    signal_cache: LruCache<String, SignalValue>,
}
```

Key responsibilities:
- Breakpoint management
- Execution state tracking
- Signal value caching
- Python mapping script integration

## Signal Mapping Integration

### Python Script Execution

DANG embeds a Python interpreter to execute signal mapping scripts:

```rust
use pyo3::prelude::*;

pub fn execute_mapping_function(
    script: &str,
    function: &str,
    args: &[PyObject]
) -> PyResult<PyObject> {
    Python::with_gil(|py| {
        let module = PyModule::from_code(py, script, "mapping.py", "mapping")?;
        let func = module.getattr(function)?;
        func.call1(args)
    })
}
```

### Signal Resolution

The mapping between Python script requests and waveform signals:

```rust
pub trait SignalMapper {
    fn get_pc(&self, time: u64) -> Result<u64>;
    fn get_registers(&self, time: u64) -> Result<HashMap<String, u64>>;
    fn get_memory(&self, time: u64, addr: u64, size: usize) -> Result<Vec<u8>>;
}
```

## Performance Optimizations

### Signal Caching

Frequently accessed signals are cached to avoid repeated waveform queries:

```rust
use lru::LruCache;

struct SignalCache {
    cache: LruCache<(String, u64), SignalValue>,
    hit_count: u64,
    miss_count: u64,
}
```

### Lazy Loading

Signals are loaded from the waveform file only when needed:

```rust
enum SignalState {
    NotLoaded,
    Loading,
    Loaded(SignalData),
    Error(String),
}
```

### Batch Operations

Multiple signal queries are batched together when possible:

```rust
pub fn batch_signal_query(
    signals: &[String],
    time: u64
) -> Result<HashMap<String, SignalValue>> {
    // Batch query implementation
}
```

## Error Handling

### Hierarchical Error Types

```rust
#[derive(thiserror::Error, Debug)]
pub enum DangError {
    #[error("Waveform loading error: {0}")]
    WaveformError(#[from] WaveformError),

    #[error("Signal mapping error: {0}")]
    MappingError(#[from] MappingError),

    #[error("GDB protocol error: {0}")]
    ProtocolError(#[from] ProtocolError),
}
```

### Recovery Strategies

- **Signal Access Errors**: Return default values and log warnings
- **Mapping Script Errors**: Continue with limited functionality
- **Protocol Errors**: Maintain connection when possible

## Configuration System

### Runtime Configuration

```rust
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub cache_size: usize,
    pub batch_size: usize,
    pub timeout_ms: u64,
    pub python_gil_timeout: Duration,
}
```

### Environment Variables

- `RUST_LOG`: Logging level override
- `DANG_CACHE_SIZE`: Signal cache size
- `DANG_BATCH_SIZE`: Batch query size
- `DANG_TIMEOUT`: Operation timeout

## Logging and Diagnostics

### Structured Logging

```rust
use tracing::{info, debug, warn, error, span, Level};

let span = span!(Level::INFO, "signal_query", signal = %signal_name);
let _enter = span.enter();

debug!("Querying signal {} at time {}", signal_name, time);
```

### Performance Metrics

```rust
#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    pub signal_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub python_calls: u64,
    pub avg_query_time: Duration,
}
```

## Testing Strategy

### Unit Tests

Each module has comprehensive unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_cache() {
        // Test implementation
    }

    #[test]
    fn test_mapping_integration() {
        // Test implementation
    }
}
```

### Integration Tests

Full system tests with sample waveform data:

```rust
#[tokio::test]
async fn test_full_debugging_session() {
    // End-to-end test implementation
}
```

### Benchmarks

Performance benchmarks for critical paths:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_signal_query(c: &mut Criterion) {
    c.bench_function("signal_query", |b| {
        b.iter(|| query_signal(black_box("cpu.pc"), black_box(1000)))
    });
}
```

## Next Steps

- [Shucks Module](./shucks.md) - GDB protocol implementation
- [Waveform Loading](./waveform-loading.md) - FST parsing details
- [Signal Mapping](../advanced/signal-mapping.md) - Python integration