fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // The engine and `std` sit next to the plugin in an installation.
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN"),
        Ok("macos") => println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path"),
        _ => {}
    }
}
