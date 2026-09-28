//! The cover image of a `.gaanim` bundle: a PNG stored as-is in the archive.
//!
//! Reading it needs no GPU, engine or Python, so file managers can show it
//! quickly: `gaanim thumbnail`, the Linux thumbnailer and the Windows
//! thumbnail handler all read it through this crate.

use std::io::{Read, Seek};
use std::path::Path;

/// Archive entry that holds the cover image.
pub const ENTRY: &str = "thumbnail.png";
/// Longest edge of the stored cover image, in pixels.
pub const SIZE: u32 = 512;
/// Class id of the Windows thumbnail handler (`gaanim_thumbnail_handler.dll`),
/// which `gaanim register` writes to the registry.
pub const WINDOWS_HANDLER_CLSID: &str = "{3595C98D-6E97-4FA6-87D7-8F87297FE898}";
/// [`WINDOWS_HANDLER_CLSID`] as the number the handler compares against.
pub const WINDOWS_HANDLER_CLSID_U128: u128 = 0x3595C98D_6E97_4FA6_87D7_8F87297FE898;
/// File name of the Windows thumbnail handler, installed next to `gaanim.exe`.
pub const WINDOWS_HANDLER_DLL: &str = "gaanim_thumbnail_handler.dll";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a .gaanim bundle: {0}")]
    NotABundle(String),
    #[error("the bundle is damaged: {0}")]
    Corrupt(String),
    #[error("could not process the image: {0}")]
    Image(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The cover PNG of the bundle in `reader`, checked against the checksum in
/// its manifest; `None` when the bundle has none (bundles from Gaanim 0.6.0).
pub fn read<R: Read + Seek>(reader: R) -> Result<Option<Vec<u8>>> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|error| Error::NotABundle(error.to_string()))?;
    let manifest = {
        let mut entry = archive
            .by_name("manifest.json")
            .map_err(|_| Error::NotABundle("no manifest.json".into()))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|error| Error::Corrupt(format!("manifest.json: {error}")))?
    };
    if manifest.get("format").and_then(|format| format.as_str()) != Some("gaanim-bundle") {
        return Err(Error::NotABundle("its manifest is not a Gaanim one".into()));
    }
    let Some(expected) = manifest
        .get("entries")
        .and_then(|entries| entries.get(ENTRY))
        .and_then(|hash| hash.as_str())
    else {
        return Ok(None);
    };
    let mut entry = archive
        .by_name(ENTRY)
        .map_err(|_| Error::Corrupt(format!("missing entry {ENTRY}")))?;
    let mut bytes = Vec::with_capacity(entry.size().min(1 << 26) as usize);
    entry.read_to_end(&mut bytes)?;
    if blake3::hash(&bytes).to_hex().as_str() != expected {
        return Err(Error::Corrupt(format!(
            "{ENTRY} does not match its checksum"
        )));
    }
    Ok(Some(bytes))
}

/// [`read`] the bundle at `path`.
pub fn read_file(path: &Path) -> Result<Option<Vec<u8>>> {
    read(std::io::BufReader::new(std::fs::File::open(path)?))
}

/// Decode `png` and shrink it so its longest edge is at most `size` pixels,
/// keeping its proportions. A smaller image is kept as it is.
pub fn scaled(png: &[u8], size: u32) -> Result<image::RgbaImage> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| Error::Image(error.to_string()))?
        .into_rgba8();
    let (width, height) = image.dimensions();
    let size = size.max(1);
    if width.max(height) <= size {
        return Ok(image);
    }
    let (width, height) = fit(width, height, size);
    Ok(image::imageops::resize(
        &image,
        width,
        height,
        image::imageops::FilterType::Triangle,
    ))
}

/// Size that fits `width × height` in a `size × size` box, at least 1 × 1.
pub fn fit(width: u32, height: u32, size: u32) -> (u32, u32) {
    let longest = width.max(height).max(1);
    let scale = |edge: u32| {
        ((u64::from(edge) * u64::from(size) + u64::from(longest) / 2) / u64::from(longest)).max(1)
            as u32
    };
    (scale(width), scale(height))
}

/// Encode `image` as PNG.
pub fn encode_png(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut bytes),
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| Error::Image(error.to_string()))?;
    Ok(bytes)
}

/// Write the cover of the bundle at `input` to the PNG file `output`, its
/// longest edge at most `size` pixels. Fails when the bundle has no cover.
pub fn extract(input: &Path, output: &Path, size: u32) -> Result<()> {
    let png = read_file(input)?.ok_or_else(|| {
        Error::NotABundle(format!(
            "{} has no cover image; record it again with this Gaanim",
            input.display()
        ))
    })?;
    let image = scaled(&png, size)?;
    std::fs::write(output, encode_png(&image)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn bundle(entries: &[(&str, &[u8])], manifest_entries: serde_json::Value) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        let manifest = serde_json::json!({
            "format": "gaanim-bundle",
            "version": 2,
            "entries": manifest_entries,
        });
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn cover(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        encode_png(&image).unwrap()
    }

    #[test]
    fn a_cover_is_read_and_checked_against_the_manifest() {
        let png = cover(512, 288);
        let hash = blake3::hash(&png).to_hex().to_string();
        let file = bundle(&[(ENTRY, &png)], serde_json::json!({ ENTRY: hash }));
        assert_eq!(read(Cursor::new(file)).unwrap(), Some(png.clone()));

        let tampered = bundle(
            &[(ENTRY, &png)],
            serde_json::json!({ ENTRY: blake3::hash(b"other").to_hex().to_string() }),
        );
        assert!(matches!(
            read(Cursor::new(tampered)),
            Err(Error::Corrupt(_))
        ));
    }

    #[test]
    fn bundles_without_a_cover_and_other_files_are_told_apart() {
        let old = bundle(&[("scene.bin", b"x")], serde_json::json!({}));
        assert_eq!(read(Cursor::new(old)).unwrap(), None);
        assert!(matches!(
            read(Cursor::new(b"not a zip".to_vec())),
            Err(Error::NotABundle(_))
        ));
    }

    #[test]
    fn covers_shrink_to_the_requested_size_and_never_grow() {
        let png = cover(512, 288);
        assert_eq!(scaled(&png, 256).unwrap().dimensions(), (256, 144));
        assert_eq!(scaled(&png, 128).unwrap().dimensions(), (128, 72));
        assert_eq!(scaled(&png, 1024).unwrap().dimensions(), (512, 288));
        let portrait = cover(288, 512);
        assert_eq!(scaled(&portrait, 256).unwrap().dimensions(), (144, 256));
        assert_eq!(fit(1920, 1080, 512), (512, 288));
        assert_eq!(fit(1, 4000, 512), (1, 512));
    }

    #[test]
    fn the_handler_class_id_has_one_value() {
        let hex = format!("{WINDOWS_HANDLER_CLSID_U128:032X}");
        let formatted = format!(
            "{{{}-{}-{}-{}-{}}}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        assert_eq!(formatted, WINDOWS_HANDLER_CLSID);
    }

    #[test]
    fn extract_writes_a_scaled_png() {
        let directory = std::env::temp_dir().join(format!("gaanim-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let png = cover(512, 288);
        let hash = blake3::hash(&png).to_hex().to_string();
        let input = directory.join("a.gaanim");
        std::fs::write(
            &input,
            bundle(&[(ENTRY, &png)], serde_json::json!({ ENTRY: hash })),
        )
        .unwrap();
        let output = directory.join("a.png");
        extract(&input, &output, 128).unwrap();
        let written = image::open(&output).unwrap();
        assert_eq!((written.width(), written.height()), (128, 72));

        let old = directory.join("old.gaanim");
        std::fs::write(&old, bundle(&[], serde_json::json!({}))).unwrap();
        assert!(extract(&old, &directory.join("b.png"), 128).is_err());
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
