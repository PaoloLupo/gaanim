fn main() {
    // Delay-load Python DLLs so the exe can start without python3*.dll on PATH
    // and we can set PATH dynamically in `python_home::ensure_python_available`
    // before `Python::initialize()` triggers the actual LoadLibrary.
    // Without this, Windows loader fails with STATUS_DLL_NOT_FOUND before main().
    // Note: python3.dll cannot be delay-loaded when using PyO3 with data symbols
    // (e.g. PyExc_ValueError), so we only delay-load versioned dlls.
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        for dll in &[
            "python312.dll",
            "python313.dll",
            "python314.dll",
            "python315.dll",
        ] {
            println!("cargo:rustc-link-arg=/DELAYLOAD:{}", dll);
        }
        println!("cargo:rustc-link-lib=delayimp");
    }
}
