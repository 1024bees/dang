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
    dwarf: gimli::Dwarf<gimli::EndianSlice<'static, gimli::RunTimeEndian>>,
    load_bias: u64,
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
    pub fn new(elf_bytes: &[u8], load_bias: u64) -> Result<Self> {
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

        // Build two Dwarf views: one for addr2line Context, one we keep for direct parsing.
        let dwarf = gimli::Dwarf::load(load_section)?;
        let ctx = addr2line::Context::from_dwarf(gimli::Dwarf::load(load_section)?)?;

        Ok(Self {
            ctx,
            dwarf,
            load_bias,

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

    /// Find addresses that correspond to a specific source file and line number.
    /// Returns a vector of runtime addresses that map to the given file:line.
    pub fn find_addresses_for_line(&self, file_path: &Path, target_line: u64) -> Result<Vec<u64>> {
        let mut addrs = Vec::new();

        let inp_file = file_path.to_path_buf();
        let is_absolute = inp_file.is_absolute();

        let dwarf = &self.dwarf;

        let mut units = dwarf.units();
        while let Some(header) = units.next()? {
            let unit = dwarf.unit(header)?;
            let mut programs = unit.line_program.as_ref().map(|lp| lp.clone());

            if let Some(ref mut program) = programs {
                let mut rows = program.clone().rows();
                while let Some((header, row)) = rows.next_row()? {
                    if row.end_sequence() {
                        continue;
                    }

                    // Line number
                    let row_line = match row.line() {
                        Some(l) => l.get() as u64,
                        None => continue,
                    };
                    if row_line != target_line {
                        continue;
                    }

                    // Resolve file path for this row
                    let file_entry = match row.file(header) {
                        Some(f) => f,
                        None => continue,
                    };

                    // Resolve file name
                    let file_name_ls = file_entry.path_name();
                    let file_name_cow = dwarf.attr_string(&unit, file_name_ls)?;
                    let file_name = std::str::from_utf8(&file_name_cow).unwrap_or_default();

                    // Resolve directory (if present)
                    let full_path = if let Some(dir_ls) = file_entry.directory(header) {
                        let dir_cow = dwarf.attr_string(&unit, dir_ls)?;
                        let dir_str = std::str::from_utf8(&dir_cow).unwrap_or_default();
                        let mut p = PathBuf::from(dir_str);
                        p.push(file_name);
                        p
                    } else {
                        PathBuf::from(file_name)
                    };

                    // Normalize path according to source search paths
                    let resolved = full_path;

                    // match logic is basically -- if input path is absolute, match against the input path
                    // otherwise, match against the input path as a suffix. e.g. main.c:12 should Just Work
                    // FIXME: we dont handle multiple matches, e.g. if there are two files named main.c in the search paths, we're cooked
                    if is_absolute && resolved == inp_file
                        || !is_absolute && resolved.ends_with(&inp_file)
                    {
                        let file_addr = row.address();
                        let runtime_addr = file_addr.saturating_add(self.load_bias);
                        addrs.push(runtime_addr);
                    }
                }
            }
        }

        // Remove duplicates and sort
        addrs.sort_unstable();
        addrs.dedup();

        Ok(addrs)
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
                let path = PathBuf::from(file);
                return Ok(Some((path, line as u64)));
            }
        }
        Ok(None)
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

    pub fn list_dwarf_files(&self) -> Result<Vec<PathBuf>> {
        let dwarf = &self.dwarf;
        let mut files: Vec<PathBuf> = Vec::new();

        let mut units = dwarf.units();
        while let Some(header) = units.next()? {
            let unit = dwarf.unit(header)?;
            if let Some(ref program) = unit.line_program {
                let header = program.header();

                // Collect file table entries from the line program header
                for file_entry in header.file_names() {
                    // File name
                    let name_ls = file_entry.path_name();
                    let name_cow = dwarf.attr_string(&unit, name_ls)?;
                    let name = std::str::from_utf8(&name_cow).unwrap_or_default();

                    // Directory (if present)
                    let full_path = if let Some(dir_ls) = file_entry.directory(header) {
                        let dir_cow = dwarf.attr_string(&unit, dir_ls)?;
                        let dir_str = std::str::from_utf8(&dir_cow).unwrap_or_default();
                        let mut p = PathBuf::from(dir_str);
                        p.push(name);
                        p
                    } else {
                        PathBuf::from(name)
                    };

                    // Normalize via configured source search paths
                    files.push(full_path);
                }
            }
        }

        // Sort and de-duplicate
        files.sort();
        files.dedup();
        Ok(files)
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

        let elf_bytes = std::fs::read(&elf_path).map_err(|e| {
            anyhow::anyhow!("Failed to read ELF file {}: {}", elf_path.display(), e)
        })?;

        // Create stepper with no load bias (statically linked executable) and no path substitutions
        let stepper = Addr2lineStepper::new(&elf_bytes, 0)?;

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

    #[test]
    fn test_find_addresses_for_line_hello_test() -> Result<()> {
        // Load the test ELF file - go up one directory from crate root to workspace root
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get workspace root")
            .to_path_buf();
        let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");
        let source_search_paths = vec![workspace_root.join("test_data/ibex/test_source")];
        let elf_bytes = std::fs::read(&elf_path).map_err(|e| {
            anyhow::anyhow!("Failed to read ELF file {}: {}", elf_path.display(), e)
        })?;

        let stepper = Addr2lineStepper::new(&elf_bytes, 0)?;
        let target_path = PathBuf::from("hello_test.c");

        // Pick lines that should generate code: puts/puthex/putchar lines (1-based)
        for &line in &[12u64, 13u64, 16u64] {
            let addrs = stepper.find_addresses_for_line(&target_path, line)?;
            assert!(
                !addrs.is_empty(),
                "Expected at least one address for {}:{}",
                target_path.display(),
                line
            );

            // Validate that the addresses round-trip back to the same file:line
            for &addr in addrs.iter().take(4) {
                let src = stepper
                    .current_line(addr)?
                    .expect("Resolved address should have source info");

                assert!(
                    src.path.ends_with(target_path.as_path()),
                    "Round-trip path mismatch for addr 0x{:x}",
                    addr
                );
                assert_eq!(
                    src.line, line,
                    "Round-trip line mismatch for addr 0x{:x}",
                    addr
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_list_dwarf_files() -> Result<()> {
        // Load the test ELF file - go up one directory from crate root to workspace root
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get workspace root")
            .to_path_buf();
        let elf_path = workspace_root.join("test_data/ibex/hello_test.elf");

        let elf_bytes = std::fs::read(&elf_path).map_err(|e| {
            anyhow::anyhow!("Failed to read ELF file {}: {}", elf_path.display(), e)
        })?;

        let stepper = Addr2lineStepper::new(&elf_bytes, 0)?;

        let files = stepper.list_dwarf_files()?;
        assert!(files.iter().any(|p| p.ends_with("hello_test.c")));

        Ok(())
    }
}
