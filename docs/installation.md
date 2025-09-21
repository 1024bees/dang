# Installation

## Building from Source

DANG is written in Rust and uses a workspace-based architecture with three main components: DANG core, JPDB utilities, and Shucks GDB protocol implementation.

### Prerequisites

- **Rust toolchain**: 1.76.0 or later
- **Git**: For cloning the repository
- **Python 3.x**: For signal mapping scripts
- **PyWellen**: Python library for waveform access

### Clone the Repository

```bash
git clone https://github.com/1024bees/dang.git
cd dang
```

### Install Python Dependencies

```bash
pip install pywellen
```

### Build the Project

```bash
# Build in release mode for best performance
cargo build --release

# Or build in debug mode for development
cargo build
```

### Install JPDB and Components

```bash
# Install the main dang binary (includes jpdb utilities)
cargo install --path dang

# Optionally install individual components
cargo install --path jpdb
cargo install --path shucks
```

This installs the binaries to your Cargo bin directory (typically `~/.cargo/bin/`).

### Verify Installation

Test your installation:

```bash
# Check version
dang --version

# Show help
dang --help
```

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
pip install pywellen numpy pandas
```

**Permission errors**:
```bash
# On Unix systems
sudo chown -R $USER ~/.cargo/
```

**PyWellen installation issues**:
```bash
# Make sure you have the correct Python version
python3 --version
pip3 install pywellen
```