JPDB: GDB for waveforms
-----------------------

JPDB is a GDB inspired debugger for debugging pre-silicon CPUs.

https://github.com/user-attachments/assets/fdd5970e-25ff-4398-96c8-e72ccc92d656


### Usage

to get started 

* a waveform 
* a python mapping file, that translates signals in the waveform
* the elf file that is being executed in the waveform

```bash
jpdb test_data/ibex/sim.fst --mapping-path test_data/ibex/signal_get.py
```

your system python must be 3.10 or newer, otherwise jpdb might bark at you 

### installation 

jpdb can be installed via cargo 

```bash 
cargo install jpdb --locked
```

the releases page on github


### mapping file

The mapping file for JPDB is the translation layer that makes signals
understandable for JPDB's internal gdb server stub. 

the mapping file MUST contain a function named `get_gdb_signals` that returns a
python `dict`. The returned python dictionary MUST contain the following keys:
* pc: signal for the current retired pc
* x0-x31: signals for each architectural general purpose register


an example mapping file is below
```python def get_gdb_signals(wave: Waveform) -> Dict[str, Signal]:
    pc = wave.get_signal_from_path(
        "TOP.ibex_simple_system.u_top.u_ibex_top.u_ibex_core.wb_stage_i.pc_wb_o"
    )
    gprs = {
        f"x{i}": wave.get_signal_from_path(
            f"TOP.ibex_simple_system.u_top.u_ibex_top.gen_regfile_ff.register_file_i.rf_reg.[{i}]"
        ).sliced(0, 31)
        for i in range(32)
    }

    rv = {"pc": pc, **gprs}
    return rv
```

To just verify that the mapping file is well formed, you can execute 

```bash
jpdb test_data/ibex/sim.fst --mapping-path test_data/ibex/signal_get.py --verify-only
```
although this will happen when you launch jpdb normally



### FAQ

* does JPDB support superscalar CPUs?

not yet, but if you give me a wave dump of a superscalar CPU, i will add support
and thank you kindly

* what instruction sets are supported?

only RV32G, but if you have a dump of another cpu that uses a different ISA, i will add
support and thank you kindly

* do i NEED to supply the elf file to use JPDB? 

at this point, yeah, we get a lot of juicy information from the elf

* n always steps into function calls whats up with that?

yeah i need to fix that sorry, ill get to it eventually or if you like the project file an issue and the guilt will accelerate me

* how does jpdb integrate with surfer? 

it uses the wave control protocol (WCP) which is nice. but also i think surfer might be a little buggy, some of the commands (e.g. adjusting viewport) cause failures while others dont. so right now the integration is fairly cursory, but the core logic is there


### Internals 

JPDB is really a few things glued together 

* dang: a GDB server for pre-sillicon CPUs
* shucks: a GDB client, written for this project specfically, with some extra hooks for interacting with waves via `wellen`
* a tui, showing the state taken out of shucks

when i was starting this out, the point was to start out with just dang and make people bring their own GDB. but two things quickly became clear: 

1. its kind of annoying to get your own gdb. i develop on a mac, and building gdb from scratch on a mac is non trivial. distributing it broadly for people to actually use also kind of sucks
2. having control over the TUI would be useful for more aggresively integrating with wave specific stuff

you can use these libaries on their own. they should _just_work_ hopefully

## acknowledgements

`wellen` library made this easy, thank you kevin laeufer

also tom verbeure did something similar a while back, shoutout
