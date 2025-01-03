# DANG: GDB for pre-silicon CPUs

Dang is uses waveforms to recreate program executions and exposes those
executions through a GDB server

# Usage

executing

```bash
dang test_data/ibex/sim.fst --mapping-path test_data/ibex/signal_get.py
```

will create a server on port 9001

you can then connect to the gdb server using lldb or gdb. using lldb you would
execute

```bash
lldb -connect connect://localhost:9001
```
