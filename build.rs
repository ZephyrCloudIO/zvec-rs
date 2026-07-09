use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const REPO: &str = "ZephyrCloudIO/zvec-rs";
const VERSION: &str = env!("CARGO_PKG_VERSION");

// DO NOT remove this rustfmt skip, the formatting is intentional so release CI can easily
// update the checksums for new releases by replacing the old ones in-place.
#[rustfmt::skip]
const CHECKSUMS: &[(&str, &str)] = &[
    ("x86_64-unknown-linux-gnu", "aa3cb531a5c7295c908926cdfc4a31f1dc3be3ed132bd1129c39c3f2dfb4ffa9"),
    ("aarch64-unknown-linux-gnu", "ac782f7b63c7782586f224572add424b5d2282d6292a75c2af25b0c2ce2ddc85"),
    ("x86_64-apple-darwin", "f366baa4dd2b5b04d3f29728fc076fe83dcd1ee443032e336096709c211679df"),
    ("aarch64-apple-darwin", "05141fc1576f0e95b0295c8b07b4bb2e781a8f2d2478f217ddaf7403de2ed8e3"),
    ("aarch64-apple-ios", "e3f3c5a89a4d242c05daa93a1fdae3b5fd47e9930a33fefdd9b762d2b6aeeed7"),
    ("aarch64-apple-ios-sim", ""),
    ("x86_64-pc-windows-msvc", "72a31705b4b416212142b72f95edd13257102b643f48a5bfe992d9b71763214e"),
    ("aarch64-linux-android", "e8720e8bdad3ededab88b77041a73c329e6d603274bcce7fc65f5c906c696c38"),
    ("x86_64-linux-android", "09eb1430ec7d90b6a40cb35584a54aedfa85a428a44cc380fb7ed6dd462dad60"),
];

fn target_triple() -> String {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    // `aarch64-apple-ios` (device) and `aarch64-apple-ios-sim` (simulator)
    // share arch/os/env and differ only by ABI. Their dylibs are NOT
    // interchangeable — ld rejects a device dylib in a simulator link
    // ("building for 'iOS-simulator', but linking in dylib built for
    // 'iOS'"), so the simulator needs its own vendor archive.
    let abi = env::var("CARGO_CFG_TARGET_ABI").unwrap_or_default();

    match (arch.as_str(), os.as_str(), env.as_str()) {
        ("x86_64", "linux", "gnu") => "x86_64-unknown-linux-gnu".into(),
        ("aarch64", "linux", "gnu") => "aarch64-unknown-linux-gnu".into(),
        ("x86_64", "macos", _) => "x86_64-apple-darwin".into(),
        ("aarch64", "macos", _) => "aarch64-apple-darwin".into(),
        ("aarch64", "ios", _) if abi == "sim" => "aarch64-apple-ios-sim".into(),
        ("aarch64", "ios", _) => "aarch64-apple-ios".into(),
        ("x86_64", "windows", "msvc") => "x86_64-pc-windows-msvc".into(),
        ("aarch64", "android", _) => "aarch64-linux-android".into(),
        ("x86_64", "android", _) => "x86_64-linux-android".into(),
        _ => panic!("unsupported target: {arch}-{os}-{env}"),
    }
}

/// Resolve an Apple SDK path via `xcrun --sdk <sdk> --show-sdk-path`.
fn apple_sdk_path(sdk: &str) -> Option<String> {
    let output = std::process::Command::new("xcrun")
        .args(["--sdk", sdk, "--show-sdk-path"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

fn lib_filename(os: &str) -> &'static str {
    match os {
        "windows" => "zvec_c_api.dll",
        "macos" | "ios" => "libzvec_c_api.dylib",
        _ => "libzvec_c_api.so",
    }
}

fn checksum_for_target(triple: &str) -> &'static str {
    CHECKSUMS
        .iter()
        .find(|(t, _)| *t == triple)
        .map(|(_, h)| *h)
        .unwrap_or_else(|| panic!("no checksum for target: {triple}"))
}

fn download_and_verify(url: &str, expected_sha256: &str, dest: &Path) {
    let resp = ureq::get(url)
        .call()
        .expect("failed to download vendor archive");
    let len: usize = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut body = Vec::with_capacity(len);
    resp.into_body()
        .as_reader()
        .read_to_end(&mut body)
        .expect("failed to read response body");

    assert!(
        !expected_sha256.is_empty(),
        "no checksum configured for {url} — run the release CI workflow"
    );
    let hash = hex::encode(Sha256::digest(&body));
    assert_eq!(
        hash, expected_sha256,
        "SHA256 mismatch for {url}: expected {expected_sha256}, got {hash}"
    );

    fs::write(dest, &body).expect("failed to write archive");
}

fn extract_tarball(archive_path: &Path, dest_dir: &Path) {
    let file = fs::File::open(archive_path).expect("failed to open archive");
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(dest_dir).expect("failed to extract archive");
}

fn copy_shared_libs_to_target(lib_dir: &Path) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let ext = match os.as_str() {
        "windows" => "dll",
        "macos" | "ios" => "dylib",
        _ => "so",
    };

    let examples_dir = target_dir.join("examples");
    let _ = fs::create_dir_all(&examples_dir);

    if let Ok(entries) = fs::read_dir(lib_dir) {
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
    println!("cargo:rerun-if-env-changed=ZVEC_LIB_DIR");

    let triple = target_triple();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let lib_dir = if let Ok(dir) = env::var("ZVEC_LIB_DIR") {
        PathBuf::from(dir)
    } else {
        let expected_sha256 = checksum_for_target(&triple);

        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let vendor_dir = out_dir.join("vendor");
        let lib_file = vendor_dir.join(lib_filename(&os));

        // Skip download if already extracted
        if !lib_file.exists() {
            let archive_name = format!("zvec_c_api-{triple}.tar.gz");
            let tag = format!("v{VERSION}");
            let url = format!("https://github.com/{REPO}/releases/download/{tag}/{archive_name}");

            let archive_path = out_dir.join(&archive_name);
            download_and_verify(&url, &expected_sha256, &archive_path);

            let _ = fs::create_dir_all(&vendor_dir);
            extract_tarball(&archive_path, &vendor_dir);
            let _ = fs::remove_file(&archive_path);
        }

        vendor_dir
    };

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=zvec_c_api");
    println!("cargo:lib_dir={}", lib_dir.display());

    copy_shared_libs_to_target(&lib_dir);

    let header = "include/zvec/c_api.h";
    println!("cargo:rerun-if-changed={header}");
    println!("cargo:rerun-if-changed=build.rs");

    let mut builder = bindgen::Builder::default()
        .header(header)
        .allowlist_function("zvec_.*")
        .allowlist_type("ZVec.*")
        .allowlist_var("ZVEC_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    // bindgen forwards the raw Rust target triple to libclang, which rejects
    // the `-sim` suffix ("version 'sim' in target triple ... is invalid")
    // and, without a sysroot, fails to find libc headers (<stdbool.h>).
    // Hand it the LLVM-style simulator triple + the iphonesimulator SDK
    // sysroot explicitly. The device triple (`aarch64-apple-ios`) is a valid
    // clang triple and needs neither.
    if triple == "aarch64-apple-ios-sim" {
        builder = builder.clang_arg("--target=arm64-apple-ios-simulator");
        if let Some(sysroot) = apple_sdk_path("iphonesimulator") {
            builder = builder.clang_arg(format!("--sysroot={sysroot}"));
        }
    }

    let bindings = builder.generate().expect("failed to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write bindings");
}
