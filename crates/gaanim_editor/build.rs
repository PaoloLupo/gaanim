fn main() {
    // The launcher activates the selected Python runtime before spawning the
    // core executable. PyO3 imports `python3.dll`; emitting delay-load flags
    // for versioned DLLs has no effect and makes MSVC report LNK4199.
    println!("cargo:rerun-if-changed=build.rs");
}
