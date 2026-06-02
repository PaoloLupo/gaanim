use bevy::prelude::Resource;
use gaanim_core::kurbo::{BezPath, Point};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, RwLock};
use ttf_parser::OutlineBuilder;

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
        self.path.quad_to(
            Point::new(x0 as f64, y0 as f64),
            Point::new(x as f64, y as f64),
        );
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

/// A central registry for TTF/OTF font files.
///
/// **System fonts** are cataloged at startup (cheap metadata scan, ~KB) but their
/// bytes are only loaded on demand when `get_font()` is called for a specific family.
/// **User-registered fonts** (via `register_font` / `register_font_file`) are stored
/// eagerly since they are explicitly opted-in.
#[derive(Resource, Debug)]
pub struct FontRegistry {
    /// User-registered fonts (explicit opt-in, eagerly loaded).
    pub registered: HashMap<String, Arc<[u8]>>,
    /// System font database (catalog only — no bytes loaded).
    db: fontdb::Database,
    /// Lazily loaded system font bytes, cached on first `get_font()` call.
    cache: RwLock<HashMap<String, Arc<[u8]>>>,
    /// Alias mappings (e.g. "sans-serif" -> "arial") built during cataloging.
    aliases: HashMap<String, String>,
}

impl Default for FontRegistry {
    fn default() -> Self {
        let mut registry = Self {
            registered: HashMap::new(),
            db: fontdb::Database::new(),
            cache: RwLock::new(HashMap::new()),
            aliases: HashMap::new(),
        };
        registry.catalog_system_fonts();
        registry
    }
}

impl FontRegistry {
    /// Creates a new `FontRegistry` and catalogs available system fonts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers font data under a specific family name (eagerly loaded).
    pub fn register_font(&mut self, family_name: impl Into<String>, bytes: Vec<u8>) {
        let name = family_name.into().to_lowercase();
        self.registered.insert(name, bytes.into());
    }

    /// Registers a font file from a local filesystem path (eagerly loaded).
    pub fn register_font_file(
        &mut self,
        family_name: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        self.register_font(family_name, bytes);
        Ok(())
    }

    /// Retrieves font bytes for a given family.
    ///
    /// Lookup order:
    /// 1. User-registered fonts (explicit `register_font` calls)
    /// 2. Lazily loaded system fonts (cached after first access)
    /// 3. System font database (loads and caches bytes on demand)
    ///
    /// Falls back to "sans-serif" then "monospace" then any available font.
    pub fn get_font(&self, family_name: &str) -> Option<Arc<[u8]>> {
        let name = family_name.to_lowercase();

        // 1. Check user-registered fonts (lock-free, immutable after init).
        if let Some(bytes) = self.registered.get(&name) {
            return Some(bytes.clone());
        }

        // 2. Check lazily loaded system font cache.
        {
            let cache = self.cache.read().unwrap();
            if let Some(bytes) = cache.get(&name) {
                return Some(bytes.clone());
            }
        }

        // 3. Try to load from system db and cache it.
        if let Some(bytes) = self.load_system_font(&name) {
            return Some(bytes);
        }

        // 4. Fallback: try monospace aliases for code/mono requests.
        if name.contains("code") || name.contains("mono") || name == "consolas" || name == "courier"
        {
            if let Some(bytes) = self.load_system_font("monospace") {
                return Some(bytes);
            }
        }

        // 5. General fallback chain: sans-serif, monospace, arial, segoe ui, any.
        for alias in &["sans-serif", "monospace", "arial", "segoe ui"] {
            if let Some(bytes) = self.load_system_font(alias) {
                return Some(bytes);
            }
        }

        // 6. Last resort: load the first available system font.
        self.load_any_system_font()
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Build the lightweight system font catalog (metadata only).
    fn catalog_system_fonts(&mut self) {
        self.db.load_system_fonts();

        for face in self.db.faces() {
            let family = face
                .families
                .first()
                .map(|(name, _)| name.as_str())
                .unwrap_or("Unknown");
            let family_lower = family.to_lowercase();

            // Build sans-serif alias: last match wins.
            if family_lower == "arial"
                || family_lower == "helvetica"
                || family_lower == "liberation sans"
                || family_lower == "dejavu sans"
                || family_lower == "segoe ui"
                || family_lower == "system-ui"
            {
                self.aliases
                    .insert("sans-serif".to_string(), family_lower.clone());
            }

            // Build monospace alias.
            if family_lower == "consolas"
                || family_lower == "courier new"
                || family_lower == "liberation mono"
                || family_lower == "dejavu sans mono"
                || family_lower == "menlo"
                || family_lower == "monaco"
                || face.monospaced
            {
                self.aliases
                    .insert("monospace".to_string(), family_lower);
            }
        }

        if self.db.faces().count() == 0 {
            bevy::prelude::warn!(
                "FontRegistry: no system fonts were found. Text rendering via rustybuzz may fail. \
                 Ensure standard font directories exist or register fonts manually."
            );
        }
    }

    /// Resolve an alias (e.g. "sans-serif") to a concrete family name.
    fn resolve_alias<'a>(&'a self, name: &'a str) -> &'a str {
        self.aliases.get(name).map_or(name, |v| v.as_str())
    }

    /// Load font bytes from the system database, insert into cache, return `Arc<[u8]>`.
    fn load_system_font(&self, name: &str) -> Option<Arc<[u8]>> {
        let resolved = self.resolve_alias(name).to_lowercase();

        // Double-check cache under read lock.
        {
            let cache = self.cache.read().unwrap();
            if let Some(bytes) = cache.get(&resolved) {
                return Some(bytes.clone());
            }
        }

        // Read from database.
        let bytes = self.read_font_bytes(&resolved)?;
        let arc: Arc<[u8]> = bytes.into();

        // Cache under both alias and resolved name.
        let mut cache = self.cache.write().unwrap();
        cache.insert(resolved.clone(), arc.clone());
        if name.to_lowercase() != resolved {
            cache.insert(name.to_lowercase(), arc.clone());
        }
        drop(cache);

        Some(arc)
    }

    /// Read font bytes for a specific family from the database.
    fn read_font_bytes(&self, family: &str) -> Option<Vec<u8>> {
        for face in self.db.faces() {
            let fam = face
                .families
                .first()
                .map(|(n, _)| n.to_lowercase())
                .unwrap_or_default();
            if fam == family {
                return match &face.source {
                    fontdb::Source::Binary(arc) => Some(arc.as_ref().as_ref().to_vec()),
                    fontdb::Source::File(path) => std::fs::read(path).ok(),
                    fontdb::Source::SharedFile(_, arc) => Some(arc.as_ref().as_ref().to_vec()),
                };
            }
        }
        None
    }

    /// Load the first available system font as a last-resort fallback.
    fn load_any_system_font(&self) -> Option<Arc<[u8]>> {
        // Try cache first.
        {
            let cache = self.cache.read().unwrap();
            if !cache.is_empty() {
                return cache.values().next().cloned();
            }
        }

        // Read the first face from the database.
        let face = self.db.faces().next()?;
        let bytes = match &face.source {
            fontdb::Source::Binary(arc) => Some(arc.as_ref().as_ref().to_vec()),
            fontdb::Source::File(path) => std::fs::read(path).ok(),
            fontdb::Source::SharedFile(_, arc) => Some(arc.as_ref().as_ref().to_vec()),
        }?;

        let family = face
            .families
            .first()
            .map(|(n, _)| n.to_lowercase())
            .unwrap_or_else(|| "unknown".to_string());

        let arc: Arc<[u8]> = bytes.into();
        let mut cache = self.cache.write().unwrap();
        cache.insert(family, arc.clone());
        Some(arc)
    }
}
