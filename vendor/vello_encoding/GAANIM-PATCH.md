# Gaanim patches to vello_encoding

## Vello image replay fix

This directory contains `vello_encoding` 0.9.0 from crates.io, originating at
Vello commit `875f324f21da93019cae9e8e61d4abfd69893206` (directory
`vello_encoding`). The upstream MIT and Apache 2.0 licenses are included.
The root `[patch.crates-io]` applies the same fix to preview and export.

This fix changes `Resolver::resolve` in `src/resolve.rs`. On a frame with no resource
patches (solid vector paths or an empty scene), `Resolver::resolve` returned
default image metadata. Vello then resized its persistent GPU image atlas to
1x1 and discarded its pixels. The resolver still considered previously used
images resident and clean, so returning to a slide with those images recreated
an empty atlas without uploading the images again.

The fast path now begins an image-cache resolve and returns the existing atlas
metadata. This preserves atlas dimensions, clears the preceding upload list,
and advances cache generations without uploading unchanged images.

Regression checks:

- `cargo test -p vello_encoding --lib`: resolver residency across empty and
  solid frames, including an atlas that grew beyond its initial size.
- `just test-package gaanim_export --lib raster_images_survive_vector_only_frames_and_replay -- --ignored`:
  actual GPU pixels across three image/vector/image playback cycles.

Remove this fix after upgrading to an upstream release that passes both
checks. Do not edit the Cargo registry cache; this copy keeps the fix
reproducible in other checkouts and release builds.

## Scene-sized GPU buffers

Upstream `BufferSizes::new` gives the bump-allocated buffers (`tiles`, `ptcl`,
`lines`, `segments`, ...) fixed sizes. When a scene needs more, every GPU stage
bails out and `fine` writes no pixel: the frame is black, or keeps the previous
frame, with no error. A few hundred paths whose bounding boxes cover a 1080p
frame are enough, because `tile_alloc` allocates every tile of each path's box.

`src/buffer_budget.rs` (new) and one call each in `src/resolve.rs` and
`src/config.rs`:

- `Resolver::resolve` records every path's device-space bounding box from the
  packed tag stream, walking it like Vello's `flatten` stage.
  `RenderConfig::new`, which Vello calls next on the same thread, sizes
  `tiles` to the tiles those boxes cover in the viewport and reserves the
  per-tile command-list space of large viewports. Scenes that fit the upstream
  sizes keep them. The walk costs about 4 ns per path tag.
- The same walk estimates the lines `flatten` makes of every segment
  (Wang's bound at Vello's 0.25 px tolerance, doubled for strokes) and the
  tile segments those lines cross. `RenderConfig::new` sizes `lines`,
  `seg_counts` and `segments` from it, so a frame of many curves (dozens of
  formulas, roughened chalk outlines) no longer exceeds the upstream 2M
  lines. This mattered most in the preview, which, unlike the exporter,
  cannot see that a frame was skipped and retry it.
- `set_bump_buffer_scale` multiplies every bump buffer and
  `set_max_bump_buffer_bytes` caps each one (default 128 MiB, wgpu's default
  `max_storage_buffer_binding_size`). Both are per thread, so the exporter
  (`crates/gaanim_export/src/gpu.rs`) raises them for its own device: it
  detects a skipped frame with sentinel probe pixels, retries it with a
  larger scale, and reports a clear error past 8×.

Regression checks:

- `cargo test -p vello_encoding --lib`: `buffer_budget` tests for path
  bounds, tile sizing, line sizing for many curves, unchanged small scenes,
  and per-thread scale/limits.
- `just test-package gaanim_export --lib gpu::`: 600 frame-spanning paths
  render without a retry; 1000 frame-sized translucent layers render after
  one.

Keep this patch while Vello sizes these buffers statically (true in 0.9 and
0.10).

## Lint expectation on `encode_brush`

Upstream marks `Encoding::encode_brush` with
`#[expect(single_use_lifetimes)]` to silence a rustc false positive
(rust-lang/rust#129255). Newer toolchains no longer report it, so the
expectation itself warned as unfulfilled on every build. It is an `allow` here,
which accepts both toolchains. Drop it with the other patches when upgrading.
