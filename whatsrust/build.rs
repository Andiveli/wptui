use std::{env, path::PathBuf, process::Command};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let status = Command::new("go")
        .env("CGO_ENABLED", "1")
        // .env("CGO_CFLAGS", "-I./lib")
        .args([
            "build",
            "-C",
            "./lib",
            "-buildmode=c-archive",
            // "-buildmode=c-shared",
            "-o",
            out_dir.join("libgo.a").to_str().unwrap(),
            // out_dir.join("libgo.so").to_str().unwrap(),
        ])
        .status()
        .unwrap();

    if !status.success() {
        panic!("Failed to build go library:\n {:?}", status);
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=lib");
    println!(
        "cargo::rustc-link-search=native={}",
        out_dir.to_str().unwrap()
    );
    println!("cargo::rustc-link-lib=static=go");
    // println!("cargo::rustc-link-lib=dylib=go");
}
