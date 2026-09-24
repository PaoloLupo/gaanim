use bevy::prelude::Resource;
use gaanim_core::kurbo::{BezPath, Point};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
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
        registry.register_embedded_scientific_font();
        registry.catalog_system_fonts();
        registry
    }
}

impl FontRegistry {
    /// Creates a new `FontRegistry` and catalogs available system fonts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry with Gaanim's embedded fonts but no system catalog.
    ///
    /// Typst resolves system fonts through its own shared store and reads only
    /// the `registered` fonts from a registry, so text measurement can skip
    /// the system font scan that [`FontRegistry::new`] performs.
    pub fn without_system_fonts() -> Self {
        let mut registry = Self {
            registered: HashMap::new(),
            db: fontdb::Database::new(),
            cache: RwLock::new(HashMap::new()),
            aliases: HashMap::new(),
        };
        registry.register_embedded_scientific_font();
        registry
    }

    /// Registers font data under a specific family name (eagerly loaded).
    pub fn register_font(&mut self, family_name: impl Into<String>, bytes: Vec<u8>) {
        self.register_font_bytes(family_name, bytes.into());
    }

    /// Registers shared font data without copying it.
    pub fn register_font_bytes(&mut self, family_name: impl Into<String>, bytes: Arc<[u8]>) {
        let name = family_name.into().to_lowercase();
        self.registered.insert(name, bytes);
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
        if (name.contains("code")
            || name.contains("mono")
            || name == "consolas"
            || name == "courier")
            && let Some(bytes) = self.load_system_font("monospace")
        {
            return Some(bytes);
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
                self.aliases.insert("monospace".to_string(), family_lower);
            }
        }

        if self.db.faces().count() == 0 {
            eprintln!(
                "FontRegistry: no system fonts were found. Text rendering via rustybuzz may fail. \
                 Ensure standard font directories exist or register fonts manually."
            );
        }
    }

    /// Register Typst's bundled New Computer Modern text face under Gaanim's
    /// stable family name. This keeps ordinary vector text consistent with
    /// equations on machines that do not have TeX fonts installed.
    fn register_embedded_scientific_font(&mut self) {
        // Located once per process; every registry shares the same bytes.
        static EMBEDDED: OnceLock<Option<Arc<[u8]>>> = OnceLock::new();
        let embedded = EMBEDDED.get_or_init(|| {
            typst_assets::fonts()
                .find(|bytes| {
                    let Ok(face) = ttf_parser::Face::parse(bytes, 0) else {
                        return false;
                    };
                    face.is_regular()
                        && face.names().into_iter().any(|name| {
                            name.is_unicode()
                                && name.to_string().is_some_and(|value| {
                                    let value = value.to_ascii_lowercase();
                                    value.contains("new computer modern")
                                        || value.contains("newcm10")
                                })
                        })
                })
                .map(Arc::from)
        });
        if let Some(bytes) = embedded {
            self.register_font_bytes("New Computer Modern", bytes.clone());
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

/// A font file found by [`scan_font_dir`], described by its first face.
#[derive(Debug, Clone)]
pub struct FontFile {
    pub path: PathBuf,
    /// Family name declared by the font (the typographic family when present).
    pub family: String,
    /// OpenType weight class, 1..=1000.
    pub weight: u16,
    pub italic: bool,
    pub bytes: Arc<[u8]>,
}

impl FontFile {
    /// A registry key that is unique per face: the bare family for the
    /// face closest to regular, `"<family> <weight>[ italic]"` otherwise.
    pub fn registry_key(&self, regular: bool) -> String {
        if regular {
            self.family.clone()
        } else if self.italic {
            format!("{} {} italic", self.family, self.weight)
        } else {
            format!("{} {}", self.family, self.weight)
        }
    }
}

/// Font file extensions recognised by [`scan_font_dir`].
pub const FONT_FILE_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

/// Read every `.ttf`, `.otf`, `.ttc` and `.otc` file directly inside `dir`
/// (not in subdirectories), sorted by path, with the family, weight and style
/// that each file declares. A file with a font extension that cannot be parsed
/// is an `InvalidData` error rather than being skipped silently.
pub fn scan_font_dir(dir: impl AsRef<Path>) -> std::io::Result<Vec<FontFile>> {
    let mut paths = std::fs::read_dir(dir.as_ref())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        FONT_FILE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
                    })
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let bytes: Arc<[u8]> = std::fs::read(&path)?.into();
            let mut database = fontdb::Database::new();
            database.load_font_data(bytes.to_vec());
            let face = database.faces().next().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("'{}' is not a readable font", path.display()),
                )
            })?;
            let family = face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_default();
            Ok(FontFile {
                family,
                weight: face.weight.0,
                italic: face.style != fontdb::Style::Normal,
                bytes,
                path,
            })
        })
        .collect()
}

/// For each family, the index of the face closest to an upright 400 weight.
pub fn regular_faces(fonts: &[FontFile]) -> Vec<bool> {
    let mut best: HashMap<&str, usize> = HashMap::new();
    let distance = |font: &FontFile| (font.italic, font.weight.abs_diff(400));
    for (index, font) in fonts.iter().enumerate() {
        best.entry(font.family.as_str())
            .and_modify(|current| {
                if distance(font) < distance(&fonts[*current]) {
                    *current = index;
                }
            })
            .or_insert(index);
    }
    (0..fonts.len())
        .map(|index| best.get(fonts[index].family.as_str()) == Some(&index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{FontRegistry, regular_faces, scan_font_dir};

    #[test]
    fn font_dir_scan_reads_family_weight_and_style_from_each_file() {
        let dir = std::env::temp_dir().join(format!("gaanim_font_dir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        let mut written = 0;
        let mut seen = std::collections::HashSet::new();
        for (index, bytes) in typst_assets::fonts().enumerate() {
            let Ok(face) = ttf_parser::Face::parse(bytes, 0) else {
                continue;
            };
            let style = (face.weight().to_number(), face.is_italic());
            if written < 3 && seen.insert(style) {
                std::fs::write(dir.join(format!("face{index}.OTF")), bytes).unwrap();
                written += 1;
            }
        }
        assert!(
            written >= 2,
            "Typst ships faces of several weights and styles"
        );
        std::fs::write(dir.join("notes.txt"), "not a font").unwrap();
        std::fs::write(dir.join("nested").join("ignored.ttf"), "nested").unwrap();

        let fonts = scan_font_dir(&dir).unwrap();
        assert_eq!(fonts.len(), written);
        assert!(fonts.windows(2).all(|pair| pair[0].path < pair[1].path));
        assert!(fonts.iter().all(|font| !font.family.is_empty()));
        let regular = regular_faces(&fonts);
        let keys = fonts
            .iter()
            .zip(&regular)
            .map(|(font, regular)| font.registry_key(*regular))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            keys.len(),
            fonts.len(),
            "every face needs its own registry key"
        );

        std::fs::write(dir.join("broken.ttf"), "not a font").unwrap();
        let error = scan_font_dir(&dir).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_without_system_fonts_keeps_the_embedded_default() {
        let full = FontRegistry::new();
        let light = FontRegistry::without_system_fonts();
        assert_eq!(
            full.registered.keys().collect::<Vec<_>>(),
            light.registered.keys().collect::<Vec<_>>(),
            "measurement and rendering registries must describe the same Typst fonts"
        );
        assert!(std::sync::Arc::ptr_eq(
            &full.registered["new computer modern"],
            &light.registered["new computer modern"]
        ));
    }

    #[test]
    fn bundled_new_computer_modern_is_resolvable() {
        let registry = FontRegistry::new();
        assert!(
            registry.registered.contains_key("new computer modern"),
            "the default must be backed by Typst's embedded New Computer Modern bytes"
        );
        assert!(
            registry.get_font("New Computer Modern").is_some(),
            "the scientific default must resolve without relying on a system font"
        );
    }
}
