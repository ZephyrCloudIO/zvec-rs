use std::env;
use std::fs;
use std::path::PathBuf;

/// Consolidate all runtime shared libs into the install dir (e.g. out/bin on windows, out/lib on unix)
/// so that DEP_ZVEC_C_API_LIB_DIR is the single source of truth for downstream consumers.
fn consolidate_shared_libs(dst: &std::path::Path, install_dir: &str) {
    let install_path = PathBuf::from(install_dir);
    let build_bin = if cfg!(target_os = "windows") {
        dst.join("build/bin")
    } else {
        dst.join("build/lib")
    };

    if let Ok(entries) = fs::read_dir(&build_bin) {
        let ext = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                let name = path.file_name().unwrap();
                let _ = fs::copy(&path, install_path.join(name));
            }
        }
    }
}

/// Copy all shared libs from the install dir into the cargo target dir
/// so examples and tests can find them at runtime.
fn copy_shared_libs_to_target(install_dir: &str) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    // OUT_DIR is target/<profile>/build/<pkg>/out — go up 3 to get target/<profile>/
    let target_dir = out_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };

    let examples_dir = target_dir.join("examples");
    let _ = fs::create_dir_all(&examples_dir);

    if let Ok(entries) = fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                let name = path.file_name().unwrap();
                let _ = fs::copy(&path, target_dir.join(name));
                let _ = fs::copy(&path, examples_dir.join(name));
            }
        }
    }
}

fn main() {
    let dst = cmake::Config::new("zvec")
        .generator("Ninja")
        .env("CMAKE_GENERATOR", "Ninja")
        .define("BUILD_C_BINDINGS", "ON")
        .define("BUILD_TOOLS", "OFF")
        .define("BUILD_PYTHON_BINDINGS", "OFF")
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .build();

    let target = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let library_dir = if target == "windows" {
        format!("{}/bin", dst.display())
    } else {
        format!("{}/lib", dst.display())
    };

    println!("cargo:rustc-link-search=native={}", library_dir);
    // on windows the .lib import library is in lib/, the .dll is in bin/
    if target == "windows" {
        println!("cargo:rustc-link-search=native={}/lib", dst.display());
    }
    println!("cargo:rustc-link-lib=dylib=zvec_c_api");

    // consolidate transitive deps into install dir, then expose it
    consolidate_shared_libs(&dst, &library_dir);
    println!("cargo:lib_dir={}", library_dir);

    copy_shared_libs_to_target(&library_dir);

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
