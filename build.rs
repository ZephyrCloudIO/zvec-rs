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
    ("x86_64-unknown-linux-gnu", "ba8819db48a97f2d8cd28f25c8fe99332c0b0bd87053ad5e7f20b259fb41c82c"),
    ("aarch64-unknown-linux-gnu", "839e486a97e9b7ea3cce1b120e083c35b2281814fa0cb32481cac68fb9600b2e"),
    ("x86_64-apple-darwin", "2caeef633fb23a3ba45df2e59cb2f7d0ec10a95d545c21b19deba2fe57a574c0"),
    ("aarch64-apple-darwin", "00b626ca8737b084f2c128a80f0f2e8f73f5112c6310483a23793afed3a0aaaa"),
    ("aarch64-apple-ios", "551705765c6cb191724df597071fdec09a1db099c73f02974e163040287b9811"),
    ("aarch64-apple-ios-sim", "a6f2820beb54dc2ea0c9a2776385d8078e2cbf5ca5a2c4456cb6279f38321a72"),
    ("x86_64-pc-windows-msvc", "229e65308285829b96203b58ed6617f758c8ee5f1fcf1ea9c4be39481f3be767"),
    ("aarch64-linux-android", "a7b99d276d69b94e111845a912bb37dcf4bcf8b6ec2611c9a9222c8f29b94082"),
    ("x86_64-linux-android", "9c85a191e627b3c80e0f19ec72f57c5e20a5065b206345c1f818fde5dc0aaa9b"),
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

    let import_library = lib_dir.join(import_library_filename(&os));
    assert!(
        import_library.is_file(),
        "zvec vendor archive is missing the expected import library: {}",
        import_library.display()
    );

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
