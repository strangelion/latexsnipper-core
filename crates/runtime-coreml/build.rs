use std::env;

fn main() {
    println!("cargo:rerun-if-changed=native/coreml_bridge.h");
    println!("cargo:rerun-if-changed=native/coreml_bridge.mm");

    let target = env::var("TARGET").expect("Cargo must provide TARGET to build scripts");
    if !target.contains("apple") {
        return;
    }

    cc::Build::new()
        .cpp(true)
        .file("native/coreml_bridge.mm")
        .flag("-fobjc-arc")
        .flag("-fblocks")
        .flag_if_supported("-std=c++17")
        .compile("latexsnipper_coreml_bridge");
    println!("cargo:rustc-link-lib=framework=CoreML");
    println!("cargo:rustc-link-lib=framework=Foundation");
}
