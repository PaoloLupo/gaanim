//! Link arguments of the Pyodide extension.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("emscripten") {
        // Pyodide loads extensions as Emscripten side modules (what PyO3's
        // `add_extension_module_link_args` sets). Only the module's
        // initializer is exported: rustc would also export the wasm-bindgen
        // descriptors of web-sys, which wgpu's GL backend pulls in on wasm32,
        // and their imports cannot be resolved outside wasm-bindgen's glue.
        // With `SIDE_MODULE=2` everything unreachable from it is dropped.
        println!("cargo:rustc-cdylib-link-arg=-sSIDE_MODULE=2");
        println!("cargo:rustc-cdylib-link-arg=-sWASM_BIGINT");
        println!("cargo:rustc-cdylib-link-arg=-sEXPORTED_FUNCTIONS=_PyInit_gaanim_core");
    }
}
