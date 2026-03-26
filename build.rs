use std::env;
use std::path::PathBuf;

fn main() {
    let dst = cmake::Config::new("zvec")
        .generator("Ninja")
        .env("CMAKE_GENERATOR", "Ninja")
        .define("BUILD_C_BINDINGS", "ON")
        .define("BUILD_TOOLS", "OFF")
        .define("BUILD_PYTHON_BINDINGS", "OFF")
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=dylib=zvec_c_api");
    println!("cargo:lib_dir={}/lib", dst.display());

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
