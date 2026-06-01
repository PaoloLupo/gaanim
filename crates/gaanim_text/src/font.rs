use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use bevy::prelude::Resource;
use ttf_parser::OutlineBuilder;
use gaanim_core::kurbo::{BezPath, Point};

/// A custom builder that collects OpenType glyph outline instructions
/// and translates them directly into a `kurbo::BezPath`.
#[derive(Default, Debug, Clone)]
pub struct OutlineCollector {
    pub path: BezPath,
}

impl OutlineCollector {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OutlineBuilder for OutlineCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(Point::new(x as f64, y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(Point::new(x as f64, y as f64));
    }

    fn quad_to(&mut self, x0: f32, y0: f32, x: f32, y: f32) {
        self.path.quad_to(Point::new(x0 as f64, y0 as f64), Point::new(x as f64, y as f64));
    }

    fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.curve_to(
            Point::new(x0 as f64, y0 as f64),
            Point::new(x1 as f64, y1 as f64),
            Point::new(x as f64, y as f64),
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

/// A central cache and registry for loaded TTF/OTF font files.
///
/// Automatically searches standard system directories to register default fonts
/// so that text and formulas render out of the box on Windows, macOS, and Linux.
#[derive(Resource, Debug, Clone)]
pub struct FontRegistry {
    pub fonts: HashMap<String, Vec<u8>>,
}

impl Default for FontRegistry {
    fn default() -> Self {
        let mut registry = Self {
            fonts: HashMap::new(),
        };

        // Try to load standard system fonts to get a robust zero-setup out of the box experience
        registry.load_system_defaults();
        registry
    }
}

impl FontRegistry {
    /// Creates a new empty `FontRegistry`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers font data under a specific family name.
    pub fn register_font(&mut self, family_name: impl Into<String>, bytes: Vec<u8>) {
        let name = family_name.into().to_lowercase();
        self.fonts.insert(name, bytes);
    }

    /// Registers a font file from a local filesystem path.
    pub fn register_font_file(&mut self, family_name: impl Into<String>, path: impl AsRef<Path>) -> std::io::Result<()> {
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        self.register_font(family_name, bytes);
        Ok(())
    }

    /// Retrieves a reference to the registered font bytes for a given family.
    /// Falls back to standard "sans-serif" or "monospace" if the font family is not registered.
    pub fn get_font(&self, family_name: &str) -> Option<&[u8]> {
        let name = family_name.to_lowercase();
        if let Some(bytes) = self.fonts.get(&name) {
            return Some(bytes.as_slice());
        }

        // If the requested font looks like a monospace/code font, try monospace first
        if name.contains("code") || name.contains("mono") || name == "consolas" || name == "courier" {
            if let Some(bytes) = self.fonts.get("monospace") {
                return Some(bytes.as_slice());
            }
        }

        // General fallback chain: try sans-serif, monospace, arial, segoe ui, and finally any available font
        self.fonts.get("sans-serif")
            .or_else(|| self.fonts.get("monospace"))
            .or_else(|| self.fonts.get("arial"))
            .or_else(|| self.fonts.get("segoe ui"))
            .or_else(|| self.fonts.values().next())
            .map(|v| v.as_slice())
    }

    /// Auto-discovers and registers default fonts from the host operating system.
    ///
    /// Uses `fontdb` to query the OS-native font database (Windows Registry,
    /// macOS CoreText, Linux fontconfig) and collect all available `.ttf`/`.otf`/`.ttc` fonts.
    ///
    /// Note: Typst default fonts (LibertinusSerif, NewCMMath, etc.) are loaded
    /// automatically by `GaanimTypstWorld` via `typst-kit`, so they do not need
    /// to be registered here. This registry is mainly for `rustybuzz` shaping
    /// and for any extra custom fonts the user registers manually.
    fn load_system_defaults(&mut self) {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        for face in db.faces() {
            let bytes = match &face.source {
                fontdb::Source::Binary(arc) => arc.as_ref().as_ref().to_vec(),
                fontdb::Source::File(path) => std::fs::read(path).unwrap_or_default(),
                fontdb::Source::SharedFile(_, arc) => arc.as_ref().as_ref().to_vec(),
            };
            if !bytes.is_empty() {
                let family = face
                    .families
                    .first()
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("Unknown");
                self.register_font(family, bytes.clone());

                // Assign standard multiplatform aliases
                let family_lower = family.to_lowercase();
                if family_lower == "arial" 
                    || family_lower == "helvetica" 
                    || family_lower == "liberation sans" 
                    || family_lower == "dejavu sans" 
                    || family_lower == "segoe ui"
                    || family_lower == "system-ui"
                {
                    self.fonts.insert("sans-serif".to_string(), bytes.clone());
                }

                if family_lower == "consolas" 
                    || family_lower == "courier new" 
                    || family_lower == "liberation mono" 
                    || family_lower == "dejavu sans mono" 
                    || family_lower == "menlo" 
                    || family_lower == "monaco"
                    || face.monospaced
                {
                    self.fonts.insert("monospace".to_string(), bytes);
                }
            }
        }

        if self.fonts.is_empty() {
            bevy::prelude::warn!(
                "FontRegistry: no system fonts were found. Text rendering via rustybuzz may fail. \
                 Ensure standard font directories exist or register fonts manually."
            );
        }
    }
}
