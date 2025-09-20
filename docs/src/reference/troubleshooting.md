# Troubleshooting

Common issues and solutions when using DANG.

## Installation Issues

### Rust Toolchain Problems
```bash
rustup update
```

### Missing Dependencies
```bash
# Install required Python packages
pip install numpy

# Install mdbook for documentation
cargo install mdbook
```

## Runtime Issues

### Waveform Loading Errors
- Verify FST file integrity
- Check file permissions
- Ensure sufficient memory

### Signal Mapping Issues
- Validate Python script syntax
- Check signal name mappings
- Review Python error logs

### Connection Problems
- Verify port availability
- Check firewall settings
- Confirm debugger compatibility

## Performance Issues

### Large Waveform Files
- Use release builds
- Increase system RAM
- Consider file compression

### Slow Signal Queries
- Optimize mapping scripts
- Use signal caching
- Batch signal operations

## Debug Tips

Enable verbose logging:
```bash
RUST_LOG=debug dang waveform.fst --mapping-path mapping.py
```

For additional help, see the [GitHub Issues](https://github.com/1024bees/dang/issues) page.