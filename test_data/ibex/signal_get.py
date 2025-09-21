from pywellen import Waveform, Signal
from typing import List, Dict


def get_gdb_signals(wave: Waveform) -> Dict[str, Signal]:
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


def get_misc_signals(wave: Waveform) -> List[Signal]:
    return [
        wave.get_signal_from_path(
            "TOP.ibex_simple_system.u_top.u_ibex_top.u_ibex_core.wb_stage_i.pc_wb_o"
        )
    ]
