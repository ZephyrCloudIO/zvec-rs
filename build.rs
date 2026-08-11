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
    ("x86_64-unknown-linux-gnu", "890ede2aead9125e8909de84815db62263fb02c76ed467e524bd9d9527f7aac7"),
    ("aarch64-unknown-linux-gnu", "06614c72c38daf5ad7fdd52182a793b37a660b482cf7948a59eb62849b937625"),
    ("x86_64-apple-darwin", "2d5e2fa40f44fa07c0f271bbcda9af68c1371b1f5d2ff6536d99a7eff63e3ce9"),
    ("aarch64-apple-darwin", "6c74981b7d6327e1a1c320bba33694523ee902d789fff3bd4c96b9d3c0f15e30"),
    ("aarch64-apple-ios", "c4f900d6eba55fa52eaa0687c53de48417c745f0165b61e2d86ca8815ff59b8f"),
    ("aarch64-apple-ios-sim", "b69afdc5454b5a8f20162abfddde053ec8fcb06b076e8d5af47237ab5906cb92"),
    ("x86_64-pc-windows-msvc", "965d573b49453f57166d72e6510b47027502e3373d6f9e8eeabc07fd22279a79"),
    ("aarch64-linux-android", "19132201f86a007275662320b6c73bd6821e793782b1098603bd7239c175f7d4"),
    ("x86_64-linux-android", "607df316ae9f4196afee3d9b6275e65ee9c67c7e5af522b26ed863074ca47840"),
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
/// Returns the resolver's own diagnostic on failure so a misconfigured
/// Xcode/Command-Line-Tools install surfaces its real cause (xcrun writes
/// "SDK ... cannot be located" to stderr) rather than a downstream clang error.
fn apple_sdk_path(sdk: &str) -> Result<String, String> {
    let output = std::process::Command::new("xcrun")
        .args(["--sdk", sdk, "--show-sdk-path"])
        .output()
        .map_err(|e| format!("failed to run xcrun: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "xcrun --sdk {sdk} --show-sdk-path failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let path = String::from_utf8(output.stdout)
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    if path.is_empty() {
        Err(format!(
            "xcrun --sdk {sdk} --show-sdk-path returned an empty path"
        ))
    } else {
        Ok(path)
    }
}

fn lib_filename(os: &str) -> &'static str {
    match os {
        "windows" => "zvec_c_api.dll",
        "macos" | "ios" => "libzvec_c_api.dylib",
        _ => "libzvec_c_api.so",
    }
}

fn import_library_filename(os: &str) -> &'static str {
    match os {
        "windows" => "zvec_c_api.lib",
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

/// Fetch `url` with bounded retries + exponential backoff. GitHub's release
/// CDN intermittently returns 5xx (a plain 504 killed a CI run on
/// 2026-07-09); a transient gateway error must not fail the whole build.
fn download_with_retries(url: &str) -> Vec<u8> {
    const MAX_ATTEMPTS: u32 = 4;
    let mut delay = std::time::Duration::from_secs(2);
    let mut last_err = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        match ureq::get(url).call() {
            Ok(resp) => {
                let len: usize = resp
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let mut body = Vec::with_capacity(len);
                match resp.into_body().as_reader().read_to_end(&mut body) {
                    Ok(_) => return body,
                    Err(e) => last_err = format!("failed to read response body: {e}"),
                }
            }
            Err(e) => last_err = e.to_string(),
        }
        if attempt < MAX_ATTEMPTS {
            eprintln!(
                "download attempt {attempt}/{MAX_ATTEMPTS} for {url} failed ({last_err}); \
                 retrying in {delay:?}"
            );
            std::thread::sleep(delay);
            delay *= 2;
        }
    }
    panic!("failed to download vendor archive after {MAX_ATTEMPTS} attempts: {last_err} ({url})");
}

fn download_and_verify(url: &str, expected_sha256: &str, dest: &Path) {
    let body = download_with_retries(url);

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
        let runtime_library = vendor_dir.join(lib_filename(&os));
        let import_library = vendor_dir.join(import_library_filename(&os));

        // A partial extraction must self-heal instead of poisoning this OUT_DIR.
        if !runtime_library.is_file() || !import_library.is_file() {
            let archive_name = format!("zvec_c_api-{triple}.tar.gz");
            let tag = format!("v{VERSION}");
            let url = format!("https://github.com/{REPO}/releases/download/{tag}/{archive_name}");

            let archive_path = out_dir.join(&archive_name);
            download_and_verify(&url, &expected_sha256, &archive_path);

            if vendor_dir.exists() {
                fs::remove_dir_all(&vendor_dir).expect("failed to clear partial vendor directory");
            }
            fs::create_dir_all(&vendor_dir).expect("failed to create vendor directory");
            extract_tarball(&archive_path, &vendor_dir);
            fs::remove_file(&archive_path).expect("failed to remove downloaded vendor archive");
        }

        vendor_dir
    };

    for required_library in [lib_filename(&os), import_library_filename(&os)] {
        let path = lib_dir.join(required_library);
        assert!(
            path.is_file(),
            "zvec vendor archive is missing required library: {}",
            path.display()
        );
    }

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
        // --target and --sysroot are applied together: a target with no
        // sysroot is a known-broken half-configuration (bindgen can't find
        // <stdbool.h>), so fail loud with xcrun's own reason instead.
        let sysroot = apple_sdk_path("iphonesimulator").unwrap_or_else(|e| {
            panic!(
                "cannot resolve the iphonesimulator SDK sysroot for bindgen: {e}\n\
                 install Xcode + the iOS Simulator platform, or run \
                 `xcodebuild -runFirstLaunch`"
            )
        });
        builder = builder
            .clang_arg("--target=arm64-apple-ios-simulator")
            .clang_arg(format!("--sysroot={sysroot}"));
    }

    let bindings = builder.generate().expect("failed to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write bindings");
}
