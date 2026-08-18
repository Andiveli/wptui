use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Use the library prebuilt with Nix if available
    let lib_path = if let Ok(go_lib) = env::var("WHATSRUST_LIBGO") {
        PathBuf::from(go_lib)
    } else {
        let output = out_dir.join("libgo.a");

        let status = Command::new("go")
            .env("CGO_ENABLED", "1")
            .args([
                "build",
                "-C",
                "./lib",
                "-buildmode=c-archive",
                "-o",
                output.to_str().unwrap(),
            ])
            .status()
            .unwrap();

        if !status.success() {
            panic!("Failed to build go library");
        }

        output
    };

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=lib");
    // Directory-level rerun-if-changed is unreliable for edits inside files,
    // so track every production Go source that feeds the archive explicitly.
    let mut go_sources = fs::read_dir("lib")
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "go")
                && !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with("_test.go"))
        })
        .collect::<Vec<_>>();
    go_sources.sort();
    for source in go_sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }
    println!("cargo:rerun-if-changed=lib/go.mod");
    println!("cargo:rerun-if-changed=lib/go.sum");
    println!(
        "cargo::rustc-link-search=native={}",
        lib_path.parent().unwrap().display()
    );
    println!("cargo::rustc-link-lib=static=go");
    // println!("cargo::rustc-link-lib=dylib=go");

    // Go's cgo runtime and crypto/x509 talk to the system trust store
    // through these frameworks when the archive is linked into a native
    // binary.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo::rustc-link-lib=framework=CoreFoundation");
        println!("cargo::rustc-link-lib=framework=Security");
        println!("cargo::rustc-link-lib=framework=SystemConfiguration");
    }
}
