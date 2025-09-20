# Contributing to DANG

We welcome contributions to DANG! This guide will help you get started with development and understand our contribution process.

## Getting Started

### Development Environment

1. **Clone the repository**:
   ```bash
   git clone https://github.com/1024bees/dang.git
   cd dang
   ```

2. **Install dependencies**:
   ```bash
   # Rust toolchain (1.76.0+)
   rustup update

   # Python for signal mapping
   python3 -m pip install numpy
   ```

3. **Build and test**:
   ```bash
   cargo build --workspace
   cargo test --workspace
   ```

### Code Style

We use standard Rust formatting and linting tools:

```bash
# Format code
cargo fmt

# Run lints
cargo clippy --workspace -- -D warnings

# Check for common issues
cargo audit
```

### Testing

Run the full test suite:

```bash
# Unit tests
cargo test --workspace

# Integration tests
cargo test --workspace --test integration

# With test coverage
cargo tarpaulin --workspace
```

## Architecture Guidelines

### Workspace Organization

- **`dang/`**: Main CLI application and core logic
- **`shucks/`**: GDB protocol implementation (keep protocol-specific)
- **`jpdb/`**: Debugging utilities and extensions

### Code Organization

- Keep modules focused and cohesive
- Use clear, descriptive names
- Document public APIs thoroughly
- Handle errors gracefully

### Dependencies

- Minimize external dependencies
- Prefer `std` over external crates when practical
- Document why specific dependencies are needed
- Keep dependency versions up to date

## Contribution Types

### Bug Fixes

1. **Create an issue** describing the bug
2. **Write a failing test** that reproduces the issue
3. **Fix the bug** and ensure the test passes
4. **Update documentation** if needed

### New Features

1. **Discuss the feature** in an issue first
2. **Write design documentation** for significant features
3. **Implement with tests** and documentation
4. **Update examples** if the feature affects usage

### Documentation

- **Code documentation**: Use `///` for public APIs
- **Architecture docs**: Update relevant `.md` files
- **Examples**: Add practical usage examples
- **Comments**: Explain complex algorithms and design decisions

## Development Workflow

### Branch Management

- **Main branch**: Always stable and ready for release
- **Feature branches**: Use descriptive names (`feature/gdb-extensions`)
- **Bug fix branches**: Include issue number (`fix/issue-123`)

### Commit Messages

Use clear, descriptive commit messages:

```
Add support for RISC-V compressed instructions

- Parse C-extension opcodes in instruction decoder
- Update register mapping for compressed registers
- Add tests for common compressed instruction patterns

Fixes #42
```

### Pull Request Process

1. **Create a draft PR** early for feedback
2. **Ensure CI passes** (tests, formatting, lints)
3. **Update documentation** for user-facing changes
4. **Request review** from maintainers
5. **Address feedback** and iterate

## Testing Guidelines

### Unit Tests

Write unit tests for all public functions:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_parsing() {
        let signal = parse_signal("cpu.pc[31:0]");
        assert_eq!(signal.name, "cpu.pc");
        assert_eq!(signal.width, 32);
    }
}
```

### Integration Tests

Create integration tests for user workflows:

```rust
#[tokio::test]
async fn test_debugging_session() {
    let server = start_test_server().await;
    let client = connect_gdb_client(&server).await;

    client.send_command("break *0x1000").await?;
    let response = client.send_command("continue").await?;

    assert!(response.contains("Breakpoint"));
}
```

### Property-Based Testing

Use property-based testing for complex logic:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_address_translation(addr in 0u64..0xFFFFFFFF) {
        let translated = translate_address(addr);
        prop_assert!(translated <= addr);
    }
}
```

## Performance Guidelines

### Benchmarking

Add benchmarks for performance-critical code:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_signal_query(c: &mut Criterion) {
    c.bench_function("signal_query", |b| {
        b.iter(|| query_signal(black_box("cpu.pc")))
    });
}

criterion_group!(benches, benchmark_signal_query);
criterion_main!(benches);
```

### Optimization

- **Profile before optimizing**: Use `cargo flamegraph`
- **Measure impact**: Benchmark before and after changes
- **Document trade-offs**: Explain performance vs. readability decisions

## Documentation Standards

### API Documentation

Document all public APIs:

```rust
/// Queries a signal value at the specified simulation time.
///
/// # Arguments
///
/// * `signal_name` - Hierarchical signal name (e.g., "cpu.pc")
/// * `time` - Simulation time in femtoseconds
///
/// # Returns
///
/// The signal value at the specified time, or an error if the signal
/// doesn't exist or the time is out of range.
///
/// # Examples
///
/// ```rust
/// let pc_value = waveform.query_signal("cpu.pc", 1000)?;
/// println!("PC at time 1000: 0x{:x}", pc_value);
/// ```
pub fn query_signal(&self, signal_name: &str, time: u64) -> Result<u64> {
    // Implementation
}
```

### Architecture Documentation

Update architecture docs for significant changes:

- **Overview**: How components fit together
- **Design decisions**: Why specific approaches were chosen
- **Trade-offs**: Benefits and limitations of the design

## Security Guidelines

### Input Validation

Always validate external inputs:

```rust
pub fn load_waveform(path: &Path) -> Result<Waveform> {
    // Validate file exists and is readable
    if !path.exists() {
        return Err(Error::FileNotFound(path.to_path_buf()));
    }

    // Validate file size is reasonable
    let metadata = path.metadata()?;
    if metadata.len() > MAX_WAVEFORM_SIZE {
        return Err(Error::FileTooLarge(metadata.len()));
    }

    // Continue with loading...
}
```

### Python Integration

Sandbox Python script execution:

```rust
// Set resource limits for Python execution
let config = PyConfig {
    max_memory: 100 * 1024 * 1024, // 100MB
    max_execution_time: Duration::from_secs(10),
    allowed_modules: vec!["math", "collections"],
};

execute_python_with_limits(script, config)?;
```

## Release Process

### Version Management

- Follow [Semantic Versioning](https://semver.org/)
- Update `Cargo.toml` version numbers
- Tag releases with `git tag v1.2.3`

### Changelog

Maintain `CHANGELOG.md` with:
- **Added**: New features
- **Changed**: Changes in existing functionality
- **Deprecated**: Soon-to-be removed features
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security improvements

### Release Checklist

- [ ] All tests pass
- [ ] Documentation is up to date
- [ ] Changelog is updated
- [ ] Version numbers are bumped
- [ ] Release notes are written

## Getting Help

### Communication Channels

- **GitHub Issues**: Bug reports and feature requests
- **GitHub Discussions**: General questions and discussions
- **Discord/Slack**: Real-time chat (if available)

### Code Review

- **Be respectful**: Assume good intentions
- **Be specific**: Point to exact lines and explain clearly
- **Be constructive**: Suggest improvements
- **Be responsive**: Address feedback promptly

## Recognition

We value all contributions and will:

- **Credit contributors** in release notes
- **Recognize significant contributions** in documentation
- **Maintain contributor guidelines** fairly and transparently

Thank you for contributing to DANG!