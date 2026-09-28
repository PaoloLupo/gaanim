//! Windows Explorer thumbnail handler for `.gaanim` files.
//!
//! An in-process COM server (`IThumbnailProvider` with
//! `IInitializeWithStream`) that `gaanim register` points
//! `HKCU\Software\Classes\.gaanim\ShellEx\{e357fccd-…}` at. Explorer runs it in
//! its isolated thumbnail process and hands it the file as a stream; the
//! handler only reads the cover PNG stored in the bundle, so it needs no GPU
//! or engine and cannot hang Explorer. On other systems this crate is empty.

#[cfg(windows)]
mod handler;
