# Vello image replay fix

This directory contains `vello_encoding` 0.9.0 from crates.io, originating at
Vello commit `875f324f21da93019cae9e8e61d4abfd69893206` (directory
`vello_encoding`). The upstream MIT and Apache 2.0 licenses are included.
The root `[patch.crates-io]` applies the same fix to preview and export.

The only source change is in `src/resolve.rs`. On a frame with no resource
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

Remove this patch after upgrading to an upstream release that passes both
checks. Do not edit the Cargo registry cache; this copy keeps the fix
reproducible in other checkouts and release builds.
