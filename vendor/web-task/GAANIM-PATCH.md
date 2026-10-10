# Gaanim patch to web-task

This directory contains `web-task` 1.1.3 from crates.io. The upstream MIT and
Apache 2.0 licenses are included. The root `[patch.crates-io]` applies it.

`bevy_tasks` depends on `web-task` for every `wasm32` target. On
`wasm32-unknown-emscripten`, which builds the Pyodide extension of the web
playground, panics unwind, and wasm-bindgen then requires the closures it
wraps to be `UnwindSafe`. The microtask closure in `src/queue.rs` captures an
`Rc` of cells, so the crate did not compile there. The patch wraps that `Rc` in
`AssertUnwindSafe`; the closure only drains the job queue, and a job that
panics leaves nothing half-updated that a later run could observe.

On `wasm32-unknown-unknown` (the web player) panics abort, so the wrapper has
no effect there.

Remove this patch after upgrading to an upstream release that builds for
`wasm32-unknown-emscripten`.
