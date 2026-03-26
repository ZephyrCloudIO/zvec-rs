use std::env;
use std::path::PathBuf;

fn main() {
    // Use Ninja everywhere (including Arrow's ExternalProject sub-builds)
    // to avoid MSVC FileTracker .tlog paths exceeding Windows MAX_PATH.
    let dst = cmake::Config::new("zvec")
        .generator("Ninja")
        .env("CMAKE_GENERATOR", "Ninja")
        .define("BUILD_C_BINDINGS", "ON")
        .define("BUILD_TOOLS", "OFF")
        .define("BUILD_PYTHON_BINDINGS", "OFF")
        .build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=dylib=zvec_c_api");

    // Copy all DLLs from the build output so they're found at runtime.
    let bin_dir = dst.join("bin");
    if bin_dir.exists() {
        let target_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
            .ancestors()
            .nth(3)
            .unwrap()
            .to_path_buf();
        for entry in std::fs::read_dir(&bin_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                let name = path.file_name().unwrap();
                let _ = std::fs::copy(&path, target_dir.join(name));
                let deps_dir = target_dir.join("deps");
                if deps_dir.exists() {
                    let _ = std::fs::copy(&path, deps_dir.join(name));
                }
                let examples_dir = target_dir.join("examples");
                if examples_dir.exists() {
                    let _ = std::fs::copy(&path, examples_dir.join(name));
                }
            }
        }
    }
    // Also copy DLLs from the intermediate build/bin (non-installed deps).
    let build_bin_dir = dst.join("build").join("bin");
    if build_bin_dir.exists() {
        let target_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
            .ancestors()
            .nth(3)
            .unwrap()
            .to_path_buf();
        for entry in std::fs::read_dir(&build_bin_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                let name = path.file_name().unwrap();
                let _ = std::fs::copy(&path, target_dir.join(name));
                let deps_dir = target_dir.join("deps");
                if deps_dir.exists() {
                    let _ = std::fs::copy(&path, deps_dir.join(name));
                }
                let examples_dir = target_dir.join("examples");
                if examples_dir.exists() {
                    let _ = std::fs::copy(&path, examples_dir.join(name));
                }
            }
        }
    }

    let header = "zvec/src/include/zvec/c_api.h";
    println!("cargo:rerun-if-changed={}", header);

    let bindings = bindgen::Builder::default()
        .header(header)
        .clang_arg(format!("-I{}/include", dst.display()))
        .allowlist_function("zvec_.*")
        .allowlist_type("ZVec.*")
        .allowlist_var("ZVEC_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("failed to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write bindings");
}
