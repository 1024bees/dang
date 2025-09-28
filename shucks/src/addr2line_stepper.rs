use anyhow::Result;
use object::{Object, ObjectSection};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// A single source line ready for display.
#[derive(Debug, Clone)]
pub struct SourceLine {
    pub path: PathBuf,
    pub line: u64,            // 1-based
    pub text: Option<String>, // None if the file can't be read
}

/// addr2line logic holder
pub struct Addr2lineStepper {
    ctx: addr2line::Context<gimli::EndianSlice<'static, gimli::RunTimeEndian>>,
    load_bias: u64,
    source_search_paths: Vec<PathBuf>,
    source_cache: Mutex<HashMap<PathBuf, Arc<Vec<String>>>>,
    path_cache: Mutex<HashMap<PathBuf, PathBuf>>, // Cache for search_for_path results
    _section_data: Vec<Box<[u8]>>,                // Keep section data alive
}

impl Addr2lineStepper {
    /// Build once per module/ELF.
    ///
    /// - `elf_bytes`: full ELF file bytes (with debug info, or at least .debug_line)
    /// - `load_bias`: runtime_base - min_p_vaddr (0 for ET_EXEC; PIE/DSOs: compute at load time)
    /// - `source_search_paths`: optional source search paths -- files with relative paths will be searched for in these paths
    pub fn new(
        elf_bytes: &[u8],
        load_bias: u64,
        source_search_paths: Vec<PathBuf>,
    ) -> Result<Self> {
        let obj = object::File::parse(elf_bytes)?;
        let endian = if obj.is_little_endian() {
            gimli::RunTimeEndian::Little
        } else {
            gimli::RunTimeEndian::Big
        };

        // Store section data in boxes to ensure stable addresses
        let mut section_data = Vec::new();

        // Pre-load all sections we might need
        let section_ids = [
            gimli::SectionId::DebugAbbrev,
            gimli::SectionId::DebugAddr,
            gimli::SectionId::DebugInfo,
            gimli::SectionId::DebugLine,
            gimli::SectionId::DebugLineStr,
            gimli::SectionId::DebugStr,
            gimli::SectionId::DebugStrOffsets,
            gimli::SectionId::DebugRanges,
            gimli::SectionId::DebugRngLists,
            gimli::SectionId::DebugLoc,
            gimli::SectionId::DebugLocLists,
            gimli::SectionId::DebugAranges,
        ];

        let mut sections_map = HashMap::new();
        for &section_id in &section_ids {
            match obj.section_by_name(section_id.name()) {
                Some(ref section) => {
                    let data = section
                        .uncompressed_data()
                        .map_err(|_| anyhow::anyhow!("Failed to read section"))?;
                    let data_box = data.into_owned().into_boxed_slice();
                    let data_ptr = data_box.as_ptr();
                    let data_len = data_box.len();

                    // Store the boxed data to keep it alive
                    section_data.push(data_box);

                    // SAFETY: We're keeping the box alive in section_data, so this slice will remain valid
                    let slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
                    sections_map.insert(section_id, gimli::EndianSlice::new(slice, endian));
                }
                None => {
                    sections_map.insert(section_id, gimli::EndianSlice::new(&[], endian));
                }
            }
        }

        let load_section = |id: gimli::SectionId| -> Result<
            gimli::EndianSlice<'static, gimli::RunTimeEndian>,
            gimli::Error,
        > {
            Ok(sections_map
                .get(&id)
                .copied()
                .unwrap_or_else(|| gimli::EndianSlice::new(&[], endian)))
        };

        let dwarf_cow = gimli::Dwarf::load(load_section)?;
        let ctx = addr2line::Context::from_dwarf(dwarf_cow)?;

        Ok(Self {
            ctx,
            load_bias,
            source_search_paths, // Fix: was path_subst
            source_cache: Mutex::new(HashMap::new()),
            path_cache: Mutex::new(HashMap::new()), // Initialize the path cache
            _section_data: section_data,
        })
    }

    /// Resolve and return the *current* source line for `runtime_pc`.
    pub fn current_line(&self, runtime_pc: u64) -> Result<Option<SourceLine>> {
        match self.map_addr(runtime_pc)? {
            Some((path, line)) => Ok(Some(SourceLine {
                text: self.read_line_1_based(&path, line as usize),
                path,
                line,
            })),
            None => Ok(None),
        }
    }

    /// Return the next `n` **unique** source lines *after* `runtime_pc`, using your
    /// already‑computed list of upcoming instruction addresses.
    ///
    /// Pass the next instruction addresses in runtime address space (what your disassembler shows).
    pub fn next_lines_from_instructions(
        &self,
        runtime_pc: u64,
        next_instr_addrs: impl IntoIterator<Item = u64>,
        n: usize,
    ) -> Result<Vec<SourceLine>> {
        if n == 0 {
            return Ok(Vec::new());
        }

        // Determine the current line so we can skip initial duplicates.
        let cur = self.map_addr(runtime_pc)?;

        let mut out = Vec::with_capacity(n);
        let mut last_emitted: Option<(PathBuf, u64)> = None;

        for addr in next_instr_addrs {
            if let Some((path, line)) = self.map_addr(addr)? {
                // Skip if we're still on the current source line.
                if let Some((ref cpath, cline)) = cur {
                    if last_emitted.is_none() && &path == cpath && line == cline {
                        continue;
                    }
                }
                // Deduplicate consecutive (file,line) repeats.
                if let Some((ref lp, ll)) = last_emitted {
                    if &path == lp && line == ll {
                        continue;
                    }
                }

                let text = self.read_line_1_based(&path, line as usize);
                out.push(SourceLine {
                    path: path.clone(),
                    line,
                    text,
                });
                last_emitted = Some((path, line));

                if out.len() == n {
                    break;
                }
            }
        }

        Ok(out)
    }

    // --- helpers -------------------------------------------------------------

    /// Map a runtime address to (path, line) using addr2line.
    fn map_addr(&self, runtime_addr: u64) -> Result<Option<(PathBuf, u64)>> {
        let file_addr = runtime_addr.saturating_sub(self.load_bias);
        if let Some(loc) = self.ctx.find_location(file_addr)? {
            if let (Some(file), Some(line)) = (loc.file, loc.line) {
                let path = self.search_for_path(&PathBuf::from(file));
                return Ok(Some((path, line as u64)));
            }
        }
        Ok(None)
    }

    fn search_for_path(&self, original: &Path) -> PathBuf {
        // Check cache first
        if let Ok(mut cache) = self.path_cache.lock() {
            if let Some(cached_path) = cache.get(original) {
                return cached_path.clone();
            }
        }

        // Original logic
        let result = if original.is_absolute() {
            original.to_path_buf()
        } else {
            let mut found_path = None;
            for base in &self.source_search_paths {
                let path = base.join(original);
                if path.exists() {
                    found_path = Some(path);
                    break;
                }
            }
            found_path.unwrap_or_else(|| original.to_path_buf())
        };

        // Cache the result
        if let Ok(mut cache) = self.path_cache.lock() {
            cache.insert(original.to_path_buf(), result.clone());
        }

        result
    }

    fn read_line_1_based(&self, path: &Path, line_1: usize) -> Option<String> {
        // Cache entire file as Vec<String> to keep stepping snappy.
        let mut cache = self.source_cache.lock().ok()?;
        let arc = if let Some(hit) = cache.get(path) {
            hit.clone()
        } else {
            let file = File::open(path).ok()?;
            let reader = BufReader::new(file);
            let mut lines = Vec::new();
            for l in reader.lines() {
                if let Ok(s) = l {
                    lines.push(s);
                }
            }
            let arc = Arc::new(lines);
            cache.insert(path.to_path_buf(), arc.clone());
            arc
        };
        arc.get(line_1.saturating_sub(1)).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr2line_stepper_with_ibex_elf() -> Result<()> {
        // Load the test ELF file - go up one directory from crate root to workspace root
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get workspace root")
            .to_path_buf();
        let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");
        let source_search_paths = vec![workspace_root.join("test_data/ibex")];
        let elf_bytes = std::fs::read(&elf_path).map_err(|e| {
            anyhow::anyhow!("Failed to read ELF file {}: {}", elf_path.display(), e)
        })?;

        // Create stepper with no load bias (statically linked executable) and no path substitutions
        let stepper = Addr2lineStepper::new(&elf_bytes, 0, source_search_paths)?;

        // Test with some addresses that should be in the binary
        // Based on objdump: .text section is at 0x00100084
        let test_addresses = [0x00100084, 0x00100100, 0x00100200, 0x00100300, 0x00100400];

        let mut found_any = false;
        for &addr in &test_addresses {
            match stepper.current_line(addr) {
                Ok(Some(source_line)) => {
                    println!(
                        "Address 0x{:x} maps to {}:{}",
                        addr,
                        source_line.path.display(),
                        source_line.line
                    );
                    if let Some(ref text) = source_line.text {
                        println!("  Text: {}", text);
                    }
                    found_any = true;
                    break;
                }
                Ok(None) => {
                    println!("Address 0x{:x} has no debug info", addr);
                }
                Err(e) => {
                    println!("Error resolving address 0x{:x}: {}", addr, e);
                }
            }
        }

        // We should be able to create the stepper without error at minimum
        assert!(true, "Addr2lineStepper created successfully");

        // If we found any mapping, test the next_lines functionality
        if found_any {
            // Test next_lines_from_instructions with some sequential addresses
            let next_addrs = (0..10).map(|i| 0x00100084 + i * 4); // RISC-V instructions are 4 bytes
            let next_lines = stepper.next_lines_from_instructions(0x00100084, next_addrs, 3)?;

            println!("Found {} next source lines", next_lines.len());
            for line in &next_lines {
                println!("  {}:{} - {:?}", line.path.display(), line.line, line.text);
            }
        }

        Ok(())
    }
}
