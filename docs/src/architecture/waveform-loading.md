# Waveform Loading

The waveform loading system is responsible for parsing FST files and providing efficient access to signal data.

## FST File Format

FST (Fast Signal Trace) is a compressed waveform format that stores:
- Signal hierarchy and names
- Time-based signal value changes
- Metadata about the simulation

## Loading Process

### 1. File Parsing

- Open FST file and validate format
- Parse signal hierarchy and types
- Build signal name lookup tables
- Extract time range information

### 2. Signal Indexing

- Create efficient data structures for signal access
- Build time-based indexes for fast queries
- Prepare signal caching infrastructure

### 3. Lazy Loading

- Load signal data on-demand
- Cache frequently accessed signals
- Manage memory usage for large files

## Performance Optimizations

### Signal Caching

- LRU cache for recently accessed signals
- Batch loading of related signals
- Predictive caching based on access patterns

### Memory Management

- Streaming access for large files
- Compressed signal representation
- Garbage collection of unused data

### Query Optimization

- Fast time-based signal lookups
- Efficient range queries
- Parallel signal processing where possible

The waveform loading system is designed to handle files ranging from small test cases to multi-gigabyte production simulations.