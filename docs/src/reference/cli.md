# Command Line Reference

Complete reference for DANG command-line options.

## Synopsis

```bash
dang [OPTIONS] <WAVEFORM_FILE>
```

## Arguments

### `<WAVEFORM_FILE>`
Path to the FST waveform file from RTL simulation.

## Options

### `--mapping-path <PATH>`
Path to Python signal mapping script.

### `--port <PORT>`
GDB server port (default: 9001).

### `--log-level <LEVEL>`
Set logging verbosity:
- `error`: Only errors
- `warn`: Warnings and errors
- `info`: General information (default)
- `debug`: Detailed debugging
- `trace`: Very verbose tracing

### `--help`
Show help information.

### `--version`
Show version information.

## Examples

```bash
# Basic usage
dang simulation.fst --mapping-path signals.py

# Custom port and verbose logging
dang simulation.fst --mapping-path signals.py --port 8080 --log-level debug
```

## Environment Variables

- `RUST_LOG`: Override log level
- `DANG_PORT`: Default server port
- `DANG_MAPPING_PATH`: Default mapping script path