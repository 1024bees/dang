// build.rs
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn static_link_python() {
    let out_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let build_dir = out_dir.join("target/python-build");

    let mut libpython_path = None;
    for candidate in &[
        "libpython3.10.a",
        "libpython3.10m.a",
        "build/libpython3.10.a",
        "build/libpython3.10m.a",
    ] {
        let p = build_dir.join(candidate);
        if p.exists() {
            libpython_path = Some(p);
            break;
        }
    }
    let libpython_path = libpython_path
        .unwrap_or_else(|| panic!("Could not find libpython3.10.a in {:?}", build_dir));

    // 3. Instruct Cargo to link the library
    // We need to pass the directory containing the library to the linker, and the library name.
    let lib_dir = libpython_path
        .parent()
        .expect("libpython3.10.a should have a parent directory");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    // The link name is "python3.10" or "python3.10m" (minus the 'lib' prefix and '.a' suffix)
    let lib_name = libpython_path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .trim_start_matches("lib")
        .trim_end_matches(".a");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static={}", lib_name);
    //    // Platform-specific linker flags
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,--export-dynamic");
        println!("cargo:rustc-link-arg=-Wl,--whole-archive");
        println!("cargo:rustc-link-arg=-lpython3.10");
        println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-all_load");
        println!("cargo:rustc-link-arg=-lpython3.10");
    }

    // Then link needed system libs:
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=util");
    println!("cargo:rustc-link-lib=dylib=c");
    println!("cargo:rustc-link-lib=dylib=z");
    println!("cargo:rustc-link-lib=dylib=expat");

    println!("cargo:rerun-if-changed=build.rs");
}

fn main() {
    //static_link_python();
}
