# Installation

## Building from Source

DANG is written in Rust and uses a workspace-based architecture. Follow these steps to build from source:

### Prerequisites

- **Rust toolchain**: 1.76.0 or later
- **Git**: For cloning the repository
- **Python 3.x**: For signal mapping scripts

### Clone the Repository

```bash
git clone https://github.com/1024bees/dang.git
cd dang
```

### Build the Project

```bash
# Build in release mode for best performance
cargo build --release

# Or build in debug mode for development
cargo build
```

### Install Globally

```bash
cargo install --path dang
```

This installs the `dang` binary to your Cargo bin directory (typically `~/.cargo/bin/`).

## Verify Installation

Test your installation:

```bash
# Check version
dang --version

# Show help
dang --help
```

## Development Setup

For contributors and advanced users:

### Workspace Structure

The project uses a Cargo workspace with three main crates:

- **`dang`**: Main CLI application and GDB server
- **`shucks`**: GDB protocol implementation and packet handling
- **`jpdb`**: Additional debugging utilities

### Development Build

```bash
# Build all workspace members
cargo build --workspace

# Run tests
cargo test --workspace

# Check code formatting
cargo fmt --check

# Run clippy lints
cargo clippy --workspace
```

### IDE Support

The project includes VS Code configuration in `.vscode/`. For the best development experience:

1. Install the Rust Analyzer extension
2. Open the workspace root in VS Code
3. Use the provided build tasks and debug configurations

## System Requirements

### Minimum Requirements

- **OS**: macOS, Linux, or Windows
- **RAM**: 4GB (waveform files can be large)
- **Storage**: 1GB for source code and dependencies

### Recommended

- **RAM**: 16GB+ for large waveform files
- **CPU**: Multi-core processor (waveform parsing is CPU-intensive)
- **Storage**: SSD for faster file I/O

## Troubleshooting

### Common Issues

**Rust toolchain too old**:
```bash
rustup update
```

**Missing Python dependencies**:
```bash
pip install numpy pandas  # If using complex signal mapping
```

**Permission errors**:
```bash
# On Unix systems
sudo chown -R $USER ~/.cargo/
```

For more issues, see [Troubleshooting](./reference/troubleshooting.md).