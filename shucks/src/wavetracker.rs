use std::path::PathBuf;

use nucleo_matcher::{Config, Matcher, Utf32Str};
use wellen::{
    simple::{read as waveread, Waveform},
    Time, TimeTableIdx, Var, WellenError,
};

use dang::waveloader::WellenSignalExt;

#[derive(Clone)]
pub enum FormattingType {
    Hex,
    Decimal,
    Binary,
}

pub struct TrackerVar {
    var: Var,
    formatting_type: FormattingType,
}

pub struct WaveformTracker {
    waveform: Waveform,
    selected_var_order: Vec<TrackerVar>,
}

impl WaveformTracker {
    pub fn new(waveform_path: PathBuf) -> Result<Self, WellenError> {
        let waveform = waveread(waveform_path)?;
        Ok(Self {
            waveform,
            selected_var_order: Vec::new(),
        })
    }

    pub fn fuzzy_match_var(&self, query: &str) -> Vec<Var> {
        let h = self.waveform.hierarchy();
        let cfg = Config::DEFAULT;
        let mut matcher = Matcher::new(cfg);
        let mut query_buf = Vec::new();
        let q = Utf32Str::new(query, &mut query_buf);

        let mut scored: Vec<(u16, Var)> = h
            .iter_vars()
            .filter_map(|v| {
                let name = v.full_name(h);
                let mut name_buf = Vec::new();
                let cand = Utf32Str::new(&name, &mut name_buf);
                matcher.fuzzy_match(cand, q).map(|score| (score, v.clone()))
            })
            .collect();

        scored.sort_by(|(sa, va), (sb, vb)| {
            sb.cmp(sa).then_with(|| {
                let na = va.full_name(h);
                let nb = vb.full_name(h);
                na.cmp(&nb)
            })
        });

        scored.into_iter().map(|(_, v)| v).collect()
    }

    pub fn select_signal(&mut self, var: Var) {
        self.waveform.load_signals(&[var.signal_ref()]);
        self.selected_var_order.push(TrackerVar {
            var,
            formatting_type: FormattingType::Hex,
        });
    }

    pub fn get_current_time_idx(&self, timetableidx: TimeTableIdx) -> Option<Time> {
        self.waveform
            .time_table()
            .get(timetableidx as usize)
            .copied()
    }

    pub fn get_scale_factor(&self, var: Var) -> &'static str {
        "ps"
    }

    pub fn get_values(&self, idx: TimeTableIdx) -> Vec<String> {
        self.selected_var_order
            .iter()
            .map(|v| {
                (
                    v.formatting_type.clone(),
                    self.waveform.get_signal(v.var.signal_ref()),
                )
            })
            .map(|v| (v.0, v.1.map(|s| s.get_val(idx))))
            .map(|v| match v.0 {
                FormattingType::Hex => v.1.map(|s| s.to_bit_string().map(bitstring_to_hex)),
                FormattingType::Decimal => v.1.map(|s| s.to_bit_string().map(bitstring_to_decimal)),
                FormattingType::Binary => v.1.map(|s| s.to_bit_string()),
            })
            .flatten()
            .map(|v| v.unwrap_or("Could not get value".to_string()))
            .collect()
    }

    pub fn get_signal_names(&self) -> Vec<String> {
        self.selected_var_order
            .iter()
            .map(|v| v.var.full_name(self.waveform.hierarchy()))
            .collect()
    }
}

fn bitstring_to_decimal<S: AsRef<str>>(bitstring: S) -> String {
    let bitstring = bitstring.as_ref();
    // Check if the bitstring contains 'x' or 'z' values
    if bitstring.contains('x')
        || bitstring.contains('z')
        || bitstring.contains('X')
        || bitstring.contains('Z')
    {
        return bitstring.to_string();
    }

    // Convert binary string to decimal
    match u64::from_str_radix(bitstring, 2) {
        Ok(decimal) => decimal.to_string(),
        Err(_) => bitstring.to_string(), // Return original if conversion fails
    }
}

fn bitstring_to_hex<S: AsRef<str>>(bitstring: S) -> String {
    let bitstring = bitstring.as_ref();
    // Check if the bitstring contains 'x' or 'z' values
    if bitstring.contains('x')
        || bitstring.contains('z')
        || bitstring.contains('X')
        || bitstring.contains('Z')
    {
        return bitstring.to_string();
    }

    // Convert binary string to hexadecimal
    match u64::from_str_radix(bitstring, 2) {
        Ok(decimal) => format!("{:x}", decimal),
        Err(_) => bitstring.to_string(), // Return original if conversion fails
    }
}
