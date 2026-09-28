//! The Gaanim engine, built once as a shared library.
//!
//! This crate has no code of its own. Building it as a `dylib` puts the crates
//! below in one library that the `gaanim` executable links at startup and the
//! Python plugin (`gaanim_python_plugin`) links when a script opens. Both refer
//! to the crates by their own names; naming this crate anywhere makes Rust take
//! them from here instead of linking a second, static copy.

pub use gaanim_animation;
pub use gaanim_api;
pub use gaanim_bundle;
pub use gaanim_core;
pub use gaanim_diff;
pub use gaanim_editor;
pub use gaanim_export;
pub use gaanim_layout;
pub use gaanim_math;
pub use gaanim_objects;
pub use gaanim_project;
pub use gaanim_renderer;
pub use gaanim_scene;
pub use gaanim_text;
pub use gaanim_thumbnail;
pub use gaanim_timeline;
pub use gaanim_visualization;
