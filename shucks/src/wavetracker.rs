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
    // Cached data for efficient fuzzy matching
    cached_vars: Vec<(Var, String)>,
    matcher: Matcher,
}

impl WaveformTracker {
    pub fn new(waveform_path: PathBuf) -> Result<Self, WellenError> {
        let waveform = waveread(waveform_path)?;

        // Pre-compute all variable names for efficient fuzzy matching
        let h = waveform.hierarchy();
        let mut cached_vars = Vec::new();

        for var in h.iter_vars() {
            let name = var.full_name(h);
            cached_vars.push((var.clone(), name));
        }

        // Create reusable matcher instance
        let matcher = Matcher::new(Config::DEFAULT);

        Ok(Self {
            waveform,
            selected_var_order: Vec::new(),
            cached_vars,
            matcher,
        })
    }

    pub fn fuzzy_match_var(&mut self, query: &str) -> Vec<(Var, String)> {
        let mut query_buf = Vec::new();
        let q = Utf32Str::new(query, &mut query_buf);

        let mut scored: Vec<(u16, usize)> = self
            .cached_vars
            .iter()
            .enumerate()
            .filter_map(|(idx, (_, name))| {
                let mut name_buf = Vec::new();
                let cand = Utf32Str::new(name, &mut name_buf);
                self.matcher.fuzzy_match(cand, q).map(|score| (score, idx))
            })
            .collect();

        scored.sort_by(|(sa, idx_a), (sb, idx_b)| {
            sb.cmp(sa).then_with(|| {
                let name_a = &self.cached_vars[*idx_a].1;
                let name_b = &self.cached_vars[*idx_b].1;
                name_a.cmp(name_b)
            })
        });

        scored
            .into_iter()
            .map(|(_, idx)| self.cached_vars[idx].clone())
            .collect()
    }

    pub fn select_signal(&mut self, var: Var) {
        self.waveform.load_signals(&[var.signal_ref()]);
        self.selected_var_order.push(TrackerVar {
            var,
            formatting_type: FormattingType::Hex,
        });
    }

    pub fn get_current_time(&self, timetableidx: TimeTableIdx) -> Option<Time> {
        self.waveform
            .time_table()
            .get(timetableidx as usize)
            .copied()
            .unwrap_or(0);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_fuzzy_match_top_and_tt() {
        // Get the path to the test FST file
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let fst_path = PathBuf::from(cargo_manifest_dir).join("../test_data/ibex/sim.fst");

        // Create WaveformTracker with the test FST file
        let mut tracker = WaveformTracker::new(fst_path).expect("Failed to load test FST waveform");

        // Test fuzzy matching on "TOP"
        let top_matches = tracker.fuzzy_match_var("TOP");
        println!("Fuzzy matches for 'TOP':");
        for (i, (var, name)) in top_matches.iter().enumerate() {
            println!("  {}: {}", i, name);
        }

        // Test fuzzy matching on "TT"
        let tt_matches = tracker.fuzzy_match_var("TT");
        println!("Fuzzy matches for 'TT':");
        for (i, (var, name)) in tt_matches.iter().enumerate() {
            println!("  {}: {}", i, name);
        }

        // Basic validation - ensure we get some matches
        assert!(!top_matches.is_empty(), "Should find matches for 'TOP'");
        assert!(!tt_matches.is_empty(), "Should find matches for 'TT'");

        // Verify that TOP matches contain signals with "top" somewhere in the name
        let has_top_related = top_matches
            .iter()
            .any(|(_, name)| name.to_lowercase().contains("top") || name.contains("TOP"));
        assert!(
            has_top_related,
            "TOP matches should contain signals with 'top' in the name"
        );

        // Test that fuzzy matching is case-insensitive and finds partial matches
        let top_lower_matches = tracker.fuzzy_match_var("top");
        assert!(
            !top_lower_matches.is_empty(),
            "Should find matches for lowercase 'top'"
        );

        // Test short pattern matching
        let t_matches = tracker.fuzzy_match_var("t");
        assert!(
            !t_matches.is_empty(),
            "Should find matches for single character 't'"
        );

        println!("✅ Fuzzy matching test passed!");
        println!("Found {} matches for 'TOP'", top_matches.len());
        println!("Found {} matches for 'TT'", tt_matches.len());
        println!("Found {} matches for 'top'", top_lower_matches.len());
        println!("Found {} matches for 't'", t_matches.len());
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
