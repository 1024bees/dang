from pywellen import Waveform


def get_signals(wave: Waveform):

    pc = wave.get_signal_from_path(
        "TOP.ibex_simple_system.u_top.u_ibex_top.u_ibex_core.wb_stage_i.pc_wb_o"
    )
    gprs = {
        f"x{i}": wave.get_signal_from_path(
            f"TOP.ibex_simple_system.u_top.u_ibex_top.gen_regfile_ff.register_file_i.rf_reg.[{i}]"
        )
        for i in range(31)
    }
    rv = {"pc": pc, **gprs}
    return rv
