//! Playback bundles: a scene or presentation recorded into one file that
//! replays without Python.
//!
//! A bundle records, at the fixed frame grid of an export, what the 2D
//! renderer draws after the timeline, updaters, reactive bindings and every
//! Python callback have run: each drawable as a [`FragmentRecipe`] and how
//! the frame composites it. Replaying builds the fragments with the same
//! [`gaanim_renderer::fragment::build_fragment`] and composites them with the
//! same code, so a bundle frame draws exactly what the scene drew.
//!
//! The file is a ZIP archive:
//!
//! - `manifest.json`: format version, summary and the BLAKE3 hash of every
//!   other entry.
//! - `scene.bin`: static data (background, post-process shaders, timeline
//!   structure, audio tracks).
//! - `tables/*.bin`: deduplicated paths, images, recipes and strings.
//! - `frames/NNNNNN.bin`: chunks of frames, each frame encoded against the
//!   one before it; a chunk starts with a whole frame.
//! - `media/*`: embedded audio files.
//!
//! [`FragmentRecipe`]: gaanim_renderer::fragment::FragmentRecipe

mod codec;
mod digest;
mod model;

use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gaanim_renderer::background::{BackgroundPaint, ShaderBackground, ShaderBackgroundRequest};
use gaanim_renderer::fragment::FragmentStore;
use gaanim_renderer::pipeline::{
    CanvasBackground, CapturedElement, SegmentBackgroundPaint, compose_captured,
};
use gaanim_renderer::post_process::PostProcessShader;
use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop, TimelineMarker};
use serde::{Deserialize, Serialize};

use codec::{Reader, Writer};
pub use digest::scene_digest;
use model::{DecodedTables, DeltaDecoder, DeltaEncoder, FrameRecord, Tables};
pub use model::{EntityKeys, Frame, PostPass};

/// Identifies the file type in `manifest.json`.
pub const FORMAT: &str = "gaanim-bundle";
/// Version of the bundle encoding this build writes and reads.
pub const VERSION: u32 = 1;
/// File extension of playback bundles.
pub const EXTENSION: &str = "gaanim";
/// Frames per chunk: one second at the default rate.
const CHUNK_FRAMES: usize = 60;

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("could not read or write the bundle: {0}")]
    Io(#[from] std::io::Error),
    #[error("the bundle archive is damaged: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("the bundle is damaged: {0}")]
    Corrupt(String),
    #[error(
        "this bundle uses format version {found}; this Gaanim reads version {VERSION}. \
         Open it with the Gaanim version that wrote it ({generator})"
    )]
    UnsupportedVersion { found: u32, generator: String },
    #[error("{0}")]
    Unsupported(String),
}

type Result<T> = std::result::Result<T, BundleError>;

// ---------------------------------------------------------------------------
// Static scene data
// ---------------------------------------------------------------------------

/// One authored scene of the timeline, for chapter navigation.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneSpan {
    pub name: String,
    pub start: f64,
    pub end: f64,
}

/// An audio track, its file embedded under `media/`.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioData {
    /// Entry name of the embedded file.
    pub media: String,
    pub start_time: f64,
    pub duration: Option<f64>,
    pub volume: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub source_offset: f64,
    pub source_duration: Option<f64>,
    pub speed: f64,
    pub looping: bool,
}

/// Everything in a bundle that does not change from frame to frame.
#[derive(Clone, Default)]
pub struct SceneData {
    /// Name shown for the bundle, usually the script's.
    pub title: String,
    /// Frame grid of the recording.
    pub fps: u32,
    /// Timeline length in seconds.
    pub duration: f64,
    /// Authored output size in pixels.
    pub output_size: (u32, u32),
    /// Color outside the frame, as an 8-bit sRGB RGBA value.
    pub clear_color: Option<[u8; 4]>,
    pub background: Option<CanvasBackground>,
    /// Shaders that post-process passes reference by index.
    pub post_shaders: Vec<PostProcessShader>,
    pub segments: Vec<SegmentMetadata>,
    pub markers: Vec<TimelineMarker>,
    pub scenes: Vec<SceneSpan>,
    pub audio: Vec<AudioData>,
}

fn write_paint(w: &mut Writer, tables: &mut Tables, paint: &BackgroundPaint) {
    match paint {
        BackgroundPaint::Brush(brush) => {
            w.u8(0);
            codec::write_brush(w, tables, brush);
        }
        BackgroundPaint::Shader(shader) => {
            w.u8(1);
            w.str(shader.source());
            codec::write_color(w, shader.fallback());
        }
    }
}

fn read_paint(r: &mut Reader<'_>, tables: &DecodedTables) -> Result<BackgroundPaint> {
    match r.u8()? {
        0 => Ok(BackgroundPaint::Brush(codec::read_brush(r, tables)?)),
        1 => {
            let source = r.str()?.to_owned();
            let fallback = codec::read_color(r)?;
            ShaderBackground::new(source, fallback)
                .map(BackgroundPaint::Shader)
                .map_err(|error| BundleError::Corrupt(error.to_string()))
        }
        _ => Err(BundleError::Corrupt("invalid background paint".into())),
    }
}

fn write_opt_f64(w: &mut Writer, value: Option<f64>) {
    w.option(value, Writer::f64);
}

impl SceneData {
    fn write(&self, w: &mut Writer, tables: &mut Tables) {
        w.str(&self.title);
        w.var(u64::from(self.fps));
        w.f64(self.duration);
        w.var(u64::from(self.output_size.0));
        w.var(u64::from(self.output_size.1));
        w.option(self.clear_color, |w, rgba| {
            for channel in rgba {
                w.u8(channel);
            }
        });
        w.option(self.background.as_ref(), |w, background| {
            write_paint(w, tables, &background.paint);
            w.len(background.segment_paints.len());
            for segment in &background.segment_paints {
                w.f64(segment.start_time);
                w.f64(segment.end_time);
                w.option(segment.paint.as_ref(), |w, paint| {
                    write_paint(w, tables, paint)
                });
                w.bool(segment.hold_at_end);
            }
            w.var(u64::from(background.pixel_size.0));
            w.var(u64::from(background.pixel_size.1));
            let bounds = &background.bounds;
            for value in [
                bounds.min.x,
                bounds.min.y,
                bounds.min.z,
                bounds.max.x,
                bounds.max.y,
                bounds.max.z,
            ] {
                w.f64(value);
            }
        });
        w.len(self.post_shaders.len());
        for shader in &self.post_shaders {
            w.str(shader.source());
            w.len(shader.uniforms().len());
            for uniform in shader.uniforms() {
                w.str(uniform);
            }
            w.option(shader.data(), |w, data| {
                w.len(data.len());
                for texel in data.iter() {
                    for value in texel {
                        w.f32(*value);
                    }
                }
            });
            w.bool(shader.bloom());
        }
        w.len(self.segments.len());
        for segment in &self.segments {
            w.var(u64::from(segment.id));
            w.str(&segment.name);
            w.option(segment.notes.as_deref(), Writer::str);
            w.f64(segment.start_time);
            w.f64(segment.end_time);
            w.len(segment.stops.len());
            for stop in &segment.stops {
                w.option(stop.name.as_deref(), Writer::str);
                w.f64(stop.time);
                write_opt_f64(w, stop.ambient);
            }
        }
        w.len(self.markers.len());
        for marker in &self.markers {
            w.str(&marker.name);
            w.f64(marker.time);
        }
        w.len(self.scenes.len());
        for scene in &self.scenes {
            w.str(&scene.name);
            w.f64(scene.start);
            w.f64(scene.end);
        }
        w.len(self.audio.len());
        for audio in &self.audio {
            w.str(&audio.media);
            w.f64(audio.start_time);
            write_opt_f64(w, audio.duration);
            w.f64(audio.volume);
            w.f64(audio.fade_in);
            w.f64(audio.fade_out);
            w.f64(audio.source_offset);
            write_opt_f64(w, audio.source_duration);
            w.f64(audio.speed);
            w.bool(audio.looping);
        }
    }

    fn read(r: &mut Reader<'_>, tables: &DecodedTables) -> Result<Self> {
        let title = r.str()?.to_owned();
        let fps = r.u32()?;
        let duration = r.f64()?;
        let output_size = (r.u32()?, r.u32()?);
        let clear_color = r.option(|r| Ok([r.u8()?, r.u8()?, r.u8()?, r.u8()?]))?;
        let background = r.option(|r| {
            let paint = read_paint(r, tables)?;
            let count = r.len()?;
            let mut segment_paints = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                segment_paints.push(SegmentBackgroundPaint {
                    start_time: r.f64()?,
                    end_time: r.f64()?,
                    paint: r.option(|r| read_paint(r, tables))?,
                    hold_at_end: r.bool()?,
                });
            }
            let pixel_size = (r.u32()?, r.u32()?);
            let bounds = gaanim_math::Bounds3D::new_3d(
                r.f64()?,
                r.f64()?,
                r.f64()?,
                r.f64()?,
                r.f64()?,
                r.f64()?,
            );
            Ok(CanvasBackground {
                paint,
                segment_paints,
                pixel_size,
                bounds,
            })
        })?;
        let shader_count = r.len()?;
        let mut post_shaders = Vec::with_capacity(shader_count.min(256));
        for _ in 0..shader_count {
            let source = r.str()?.to_owned();
            let uniform_count = r.len()?;
            let mut uniforms = Vec::with_capacity(uniform_count.min(256));
            for _ in 0..uniform_count {
                uniforms.push(Arc::<str>::from(r.str()?));
            }
            let data = r.option(|r| {
                let count = r.len()?;
                let mut data = Vec::with_capacity(count.min(1 << 20));
                for _ in 0..count {
                    data.push([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
                }
                Ok(Arc::<[[f32; 4]]>::from(data))
            })?;
            let bloom = r.bool()?;
            post_shaders.push(
                PostProcessShader::from_parts(source, &uniforms, data, bloom)
                    .map_err(|error| BundleError::Corrupt(error.to_string()))?,
            );
        }
        let segment_count = r.len()?;
        let mut segments = Vec::with_capacity(segment_count.min(4096));
        for _ in 0..segment_count {
            let id = r.u32()?;
            let name = r.str()?.to_owned();
            let notes = r.option(|r| Ok(r.str()?.to_owned()))?;
            let start_time = r.f64()?;
            let end_time = r.f64()?;
            let stop_count = r.len()?;
            let mut stops = Vec::with_capacity(stop_count.min(4096));
            for _ in 0..stop_count {
                stops.push(SegmentStop {
                    name: r.option(|r| Ok(r.str()?.to_owned()))?,
                    time: r.f64()?,
                    ambient: r.option(Reader::f64)?,
                });
            }
            segments.push(SegmentMetadata {
                id,
                name,
                notes,
                start_time,
                end_time,
                stops,
            });
        }
        let marker_count = r.len()?;
        let mut markers = Vec::with_capacity(marker_count.min(4096));
        for _ in 0..marker_count {
            markers.push(TimelineMarker {
                name: r.str()?.to_owned(),
                time: r.f64()?,
            });
        }
        let scene_count = r.len()?;
        let mut scenes = Vec::with_capacity(scene_count.min(4096));
        for _ in 0..scene_count {
            scenes.push(SceneSpan {
                name: r.str()?.to_owned(),
                start: r.f64()?,
                end: r.f64()?,
            });
        }
        let audio_count = r.len()?;
        let mut audio = Vec::with_capacity(audio_count.min(256));
        for _ in 0..audio_count {
            audio.push(AudioData {
                media: r.str()?.to_owned(),
                start_time: r.f64()?,
                duration: r.option(Reader::f64)?,
                volume: r.f64()?,
                fade_in: r.f64()?,
                fade_out: r.f64()?,
                source_offset: r.f64()?,
                source_duration: r.option(Reader::f64)?,
                speed: r.f64()?,
                looping: r.bool()?,
            });
        }
        Ok(Self {
            title,
            fps,
            duration,
            output_size,
            clear_color,
            background,
            post_shaders,
            segments,
            markers,
            scenes,
            audio,
        })
    }
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// Human-readable summary and integrity table, stored as `manifest.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    /// Program that wrote the bundle, e.g. `gaanim 0.5.2`.
    pub generator: String,
    pub title: String,
    pub fps: u32,
    pub duration: f64,
    pub frames: usize,
    pub size: (u32, u32),
    pub segments: usize,
    pub stops: usize,
    /// Frames per chunk entry and the time of each chunk's first frame.
    pub chunks: Vec<ChunkInfo>,
    /// BLAKE3 hash (hex) of every other entry.
    pub entries: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub entry: String,
    pub first_frame: usize,
    pub frames: usize,
    /// Time of the chunk's first frame.
    pub start: f64,
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Streams frames into a bundle file.
pub struct BundleWriter<W: Write + Seek> {
    zip: zip::ZipWriter<W>,
    tables: Tables,
    keys: EntityKeys,
    delta: DeltaEncoder,
    chunk: Writer,
    chunk_frames: usize,
    chunks: Vec<ChunkInfo>,
    frames: usize,
    times: Vec<f64>,
    digests: Vec<[u8; 32]>,
    last_time: f64,
    entries: std::collections::BTreeMap<String, String>,
    media: HashMap<blake3::Hash, String>,
    generator: String,
}

fn entry_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .large_file(true)
}

impl BundleWriter<std::io::BufWriter<std::fs::File>> {
    /// Create `path`, replacing it if it exists.
    pub fn create(path: &Path, generator: impl Into<String>) -> Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self::new(std::io::BufWriter::new(file), generator))
    }
}

impl<W: Write + Seek> BundleWriter<W> {
    pub fn new(output: W, generator: impl Into<String>) -> Self {
        Self {
            zip: zip::ZipWriter::new(output),
            tables: Tables::default(),
            keys: EntityKeys::default(),
            delta: DeltaEncoder::default(),
            chunk: Writer::new(),
            chunk_frames: 0,
            chunks: Vec::new(),
            frames: 0,
            times: Vec::new(),
            digests: Vec::new(),
            last_time: f64::NEG_INFINITY,
            entries: Default::default(),
            media: HashMap::new(),
            generator: generator.into(),
        }
    }

    fn write_entry(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        self.zip.start_file(name, entry_options())?;
        self.zip.write_all(bytes)?;
        self.entries
            .insert(name.to_owned(), blake3::hash(bytes).to_hex().to_string());
        Ok(())
    }

    /// Embed a media file and return its entry name. Identical files are
    /// stored once.
    pub fn add_media(&mut self, source: &Path) -> Result<String> {
        let bytes = std::fs::read(source)?;
        let hash = blake3::hash(&bytes);
        if let Some(entry) = self.media.get(&hash) {
            return Ok(entry.clone());
        }
        let extension = source
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| extension.chars().all(|c| c.is_ascii_alphanumeric()))
            .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
            .unwrap_or_default();
        let entry = format!("media/{}{extension}", &hash.to_hex()[..32]);
        // Media is usually compressed already.
        self.zip.start_file(
            &entry,
            entry_options().compression_method(zip::CompressionMethod::Stored),
        )?;
        self.zip.write_all(&bytes)?;
        self.entries
            .insert(entry.clone(), hash.to_hex().to_string());
        self.media.insert(hash, entry.clone());
        Ok(entry)
    }

    /// Append the next frame. Frames must come in increasing time order.
    /// `digest` is [`frame_digest`] of the frame as the scene drew it, and
    /// `lottie` names the recorded Lottie frame an element draws, if any.
    pub fn push_frame(
        &mut self,
        frame: &Frame,
        digest: [u8; 32],
        lottie: impl Fn(&CapturedElement) -> Option<u32>,
    ) -> Result<()> {
        if frame.time.is_nan() || frame.time <= self.last_time {
            return Err(BundleError::Unsupported(format!(
                "frames must be recorded in increasing time order ({} after {})",
                frame.time, self.last_time
            )));
        }
        if frame
            .capture
            .elements
            .iter()
            .any(|element| element.recipe.lottie && lottie(element).is_none())
        {
            return Err(BundleError::Unsupported(
                "a Lottie animation could not be recorded into the bundle".into(),
            ));
        }
        self.last_time = frame.time;
        self.times.push(frame.time);
        self.digests.push(digest);
        let record = FrameRecord::capture(frame, &mut self.tables, &mut self.keys, lottie);
        if self.chunk_frames == 0 {
            self.chunks.push(ChunkInfo {
                entry: format!("frames/{:06}.bin", self.chunks.len()),
                first_frame: self.frames,
                frames: 0,
                start: frame.time,
            });
        }
        self.delta.write(&mut self.chunk, record);
        self.chunk_frames += 1;
        self.frames += 1;
        if self.chunk_frames == CHUNK_FRAMES {
            self.flush_chunk()?;
        }
        Ok(())
    }

    fn flush_chunk(&mut self) -> Result<()> {
        if self.chunk_frames == 0 {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.chunk).into_bytes();
        let info = self.chunks.last_mut().expect("chunk started");
        info.frames = self.chunk_frames;
        let entry = info.entry.clone();
        self.write_entry(&entry, &bytes)?;
        self.chunk_frames = 0;
        self.delta.reset();
        self.tables.release_unused_recipes();
        Ok(())
    }

    /// Write the tables, the static data and the manifest, and close the file.
    pub fn finish(mut self, scene: &SceneData) -> Result<W> {
        self.flush_chunk()?;
        let mut w = Writer::new();
        scene.write(&mut w, &mut self.tables);
        let scene_bytes = w.into_bytes();

        // Tables last: frames and static data may have added to them.
        let mut paths = Writer::new();
        paths.len(self.tables.paths.len());
        for path in &self.tables.paths {
            codec::write_path(&mut paths, path);
        }
        let mut images = Writer::new();
        images.len(self.tables.images.len());
        for image in &self.tables.images {
            codec::write_image_data(&mut images, image);
        }
        let mut recipes = Writer::new();
        recipes.len(self.tables.recipes.len());
        for recipe in &self.tables.recipes {
            recipes.bytes(recipe);
        }
        let mut strings = Writer::new();
        strings.len(self.tables.strings.len());
        for string in &self.tables.strings {
            strings.str(string);
        }
        self.write_entry("tables/paths.bin", &paths.into_bytes())?;
        self.write_entry("tables/images.bin", &images.into_bytes())?;
        self.write_entry("tables/recipes.bin", &recipes.into_bytes())?;
        self.write_entry("tables/strings.bin", &strings.into_bytes())?;
        self.write_entry("scene.bin", &scene_bytes)?;
        let mut index = Writer::new();
        index.len(self.times.len());
        for time in &self.times {
            index.f64(*time);
        }
        self.write_entry("index.bin", &index.into_bytes())?;
        let mut digests = Vec::with_capacity(self.digests.len() * 32);
        for digest in &self.digests {
            digests.extend_from_slice(digest);
        }
        self.write_entry("digests.bin", &digests)?;

        let manifest = Manifest {
            format: FORMAT.into(),
            version: VERSION,
            generator: self.generator.clone(),
            title: scene.title.clone(),
            fps: scene.fps,
            duration: scene.duration,
            frames: self.frames,
            size: scene.output_size,
            segments: scene.segments.len(),
            stops: scene
                .segments
                .iter()
                .map(|segment| segment.stops.len())
                .sum(),
            chunks: self.chunks.clone(),
            entries: self.entries.clone(),
        };
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| BundleError::Corrupt(error.to_string()))?;
        self.zip.start_file("manifest.json", entry_options())?;
        self.zip.write_all(&json)?;
        Ok(self.zip.finish()?)
    }
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// An open bundle. Frames are decoded chunk by chunk on demand.
pub struct Bundle {
    archive: zip::ZipArchive<std::io::Cursor<Arc<[u8]>>>,
    pub manifest: Manifest,
    pub scene: SceneData,
    tables: DecodedTables,
    /// Time of every frame, in order.
    times: Vec<f64>,
    /// [`frame_digest`] of every frame as the scene drew it while recording.
    digests: Vec<[u8; 32]>,
    /// Decoded chunk: index and its frames.
    cached: Option<(usize, Vec<FrameRecord>)>,
}

fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    manifest: Option<&Manifest>,
    name: &str,
) -> Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| BundleError::Corrupt(format!("missing entry {name}")))?;
    let mut bytes = Vec::with_capacity(file.size().min(1 << 30) as usize);
    file.read_to_end(&mut bytes)?;
    if let Some(manifest) = manifest {
        let expected = manifest
            .entries
            .get(name)
            .ok_or_else(|| BundleError::Corrupt(format!("entry {name} is not in the manifest")))?;
        if blake3::hash(&bytes).to_hex().as_str() != expected {
            return Err(BundleError::Corrupt(format!(
                "entry {name} does not match its checksum"
            )));
        }
    }
    Ok(bytes)
}

impl Bundle {
    /// Open and validate a bundle file.
    pub fn open(path: &Path) -> Result<Self> {
        let bytes: Arc<[u8]> = std::fs::read(path)?.into();
        Self::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Arc<[u8]>) -> Result<Self> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        let manifest: Manifest =
            serde_json::from_slice(&read_entry(&mut archive, None, "manifest.json")?)
                .map_err(|error| BundleError::Corrupt(format!("manifest.json: {error}")))?;
        if manifest.format != FORMAT {
            return Err(BundleError::Corrupt(format!(
                "not a Gaanim bundle (format \"{}\")",
                manifest.format
            )));
        }
        if manifest.version != VERSION {
            return Err(BundleError::UnsupportedVersion {
                found: manifest.version,
                generator: manifest.generator.clone(),
            });
        }

        let mut tables = DecodedTables {
            paths: Vec::new(),
            images: Vec::new(),
            recipes: Vec::new(),
            strings: Vec::new(),
        };
        let paths = read_entry(&mut archive, Some(&manifest), "tables/paths.bin")?;
        let mut r = Reader::new(&paths);
        for _ in 0..r.len()? {
            tables.paths.push(Arc::new(codec::read_path(&mut r)?));
        }
        let images = read_entry(&mut archive, Some(&manifest), "tables/images.bin")?;
        let mut r = Reader::new(&images);
        for _ in 0..r.len()? {
            tables.images.push(codec::read_image_data(&mut r)?);
        }
        let strings = read_entry(&mut archive, Some(&manifest), "tables/strings.bin")?;
        let mut r = Reader::new(&strings);
        for _ in 0..r.len()? {
            tables.strings.push(Arc::from(r.str()?));
        }
        let recipes = read_entry(&mut archive, Some(&manifest), "tables/recipes.bin")?;
        let mut r = Reader::new(&recipes);
        let mut encoded = Vec::new();
        for _ in 0..r.len()? {
            encoded.push(r.bytes()?.to_vec());
        }
        model::decode_recipes(&encoded, &mut tables)?;
        let scene_bytes = read_entry(&mut archive, Some(&manifest), "scene.bin")?;
        let scene = SceneData::read(&mut Reader::new(&scene_bytes), &tables)?;

        let index = read_entry(&mut archive, Some(&manifest), "index.bin")?;
        let mut r = Reader::new(&index);
        let count = r.len()?;
        let mut times = Vec::with_capacity(count.min(1 << 24));
        for _ in 0..count {
            times.push(r.f64()?);
        }
        let chunked: usize = manifest.chunks.iter().map(|chunk| chunk.frames).sum();
        if times.len() != manifest.frames
            || chunked != manifest.frames
            || times.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(BundleError::Corrupt("frame index is inconsistent".into()));
        }
        let digest_bytes = read_entry(&mut archive, Some(&manifest), "digests.bin")?;
        if digest_bytes.len() != times.len() * 32 {
            return Err(BundleError::Corrupt(
                "frame digests are inconsistent".into(),
            ));
        }
        let digests = digest_bytes
            .chunks_exact(32)
            .map(|digest| digest.try_into().expect("32-byte digest"))
            .collect();
        Ok(Self {
            archive,
            manifest,
            scene,
            tables,
            times,
            digests,
            cached: None,
        })
    }

    fn decode_chunk(&mut self, chunk: usize) -> Result<Vec<FrameRecord>> {
        let info = self
            .manifest
            .chunks
            .get(chunk)
            .ok_or_else(|| BundleError::Corrupt("chunk index out of range".into()))?
            .clone();
        let bytes = read_entry(&mut self.archive, Some(&self.manifest), &info.entry)?;
        let mut r = Reader::new(&bytes);
        let mut decoder = DeltaDecoder::default();
        let mut frames = Vec::with_capacity(info.frames);
        for _ in 0..info.frames {
            frames.push(decoder.read(&mut r)?);
        }
        if !r.is_empty() {
            return Err(BundleError::Corrupt(format!(
                "{} has trailing data",
                info.entry
            )));
        }
        Ok(frames)
    }

    /// Number of recorded frames.
    pub fn frame_count(&self) -> usize {
        self.times.len()
    }

    /// Time of every recorded frame, in order.
    pub fn times(&self) -> &[f64] {
        &self.times
    }

    /// The frame shown at `time`: the last one recorded at or before it.
    pub fn frame_index_at(&self, time: f64) -> usize {
        // Tolerate the rounding of accumulated frame steps.
        let time = time + 1e-9;
        self.times
            .partition_point(|frame_time| *frame_time <= time)
            .saturating_sub(1)
    }

    /// Decode frame `index`.
    pub fn frame(&mut self, index: usize) -> Result<Frame> {
        let chunk = self
            .manifest
            .chunks
            .partition_point(|info| info.first_frame <= index)
            .checked_sub(1)
            .ok_or_else(|| BundleError::Corrupt("frame index out of range".into()))?;
        if self
            .cached
            .as_ref()
            .is_none_or(|(cached, _)| *cached != chunk)
        {
            let frames = self.decode_chunk(chunk)?;
            self.cached = Some((chunk, frames));
        }
        let (_, frames) = self.cached.as_ref().expect("chunk decoded");
        let first = self.manifest.chunks[chunk].first_frame;
        let record = frames
            .get(index - first)
            .ok_or_else(|| BundleError::Corrupt("frame index out of range".into()))?;
        record.resolve(&self.tables, |_| {
            Err(BundleError::Unsupported(
                "this bundle draws Lottie animations, which this player cannot show yet".into(),
            ))
        })
    }

    /// [`frame_digest`] recorded for frame `index`.
    pub fn digest(&self, index: usize) -> Option<[u8; 32]> {
        self.digests.get(index).copied()
    }

    /// Compose every frame again and compare it with the digest recorded
    /// for it. Returns the frames that differ, by index and time.
    pub fn verify(&mut self) -> Result<Vec<(usize, f64)>> {
        let background = self.scene.background.clone();
        let mut store = FragmentStore::default();
        let mut mismatched = Vec::new();
        for index in 0..self.frame_count() {
            let frame = self.frame(index)?;
            if frame_digest(&frame, background.as_ref(), &mut store) != self.digests[index] {
                mismatched.push((index, frame.time));
            }
            store.end_frame();
        }
        Ok(mismatched)
    }

    /// Bytes of an embedded media entry.
    pub fn media(&mut self, entry: &str) -> Result<Vec<u8>> {
        read_entry(&mut self.archive, Some(&self.manifest), entry)
    }

    /// Write the embedded media into `dir` (content-addressed, so repeated
    /// extraction reuses files) and return the path of every entry.
    pub fn extract_media(&mut self, dir: &Path) -> Result<HashMap<String, PathBuf>> {
        let mut paths = HashMap::new();
        let entries: Vec<String> = self
            .manifest
            .entries
            .keys()
            .filter(|entry| entry.starts_with("media/"))
            .cloned()
            .collect();
        for entry in entries {
            let name = entry.trim_start_matches("media/");
            let path = dir.join(name);
            if !path.is_file() {
                std::fs::create_dir_all(dir)?;
                let bytes = self.media(&entry)?;
                let temporary = dir.join(format!("{name}.tmp"));
                std::fs::write(&temporary, bytes)?;
                std::fs::rename(&temporary, &path)?;
            }
            paths.insert(entry, path);
        }
        Ok(paths)
    }
}

/// Composite a frame as the preview draws it: over `background` (left out
/// in perspective), with a shader background returned as a GPU request.
/// Opacity layers are padded for the background's pixel size, as the scene
/// pads them when it renders at that size.
pub fn compose_frame(
    frame: &Frame,
    background: Option<&CanvasBackground>,
    store: &mut FragmentStore,
) -> (vello::Scene, Option<ShaderBackgroundRequest>) {
    let perspective = matches!(
        frame.camera.projection,
        gaanim_math::Projection::Perspective { .. }
    );
    let mut request = None;
    let pixels_per_unit = background.and_then(|background| {
        gaanim_renderer::pipeline::output_pixels_per_unit(&frame.camera, background.pixel_size.0)
    });
    let scene = compose_captured(
        &frame.capture,
        store,
        background
            .filter(|_| !perspective)
            .map(|background| (background, background.pixel_size)),
        pixels_per_unit,
        Some(&mut request),
    );
    (scene, request)
}

/// Digest of what a frame hands the renderer: its composed scene, the
/// shader background it requests, its camera and its post-processing.
pub fn frame_digest(
    frame: &Frame,
    background: Option<&CanvasBackground>,
    store: &mut FragmentStore,
) -> [u8; 32] {
    let (scene, request) = compose_frame(frame, background, store);
    let mut hasher = blake3::Hasher::new();
    hasher.update(&scene_digest(&scene));
    if let Some(request) = request {
        hasher.update(&request.time().to_bits().to_le_bytes());
        for value in request.frame() {
            hasher.update(&value.to_bits().to_le_bytes());
        }
        hasher.update(&request.image().width.to_le_bytes());
        hasher.update(&request.image().height.to_le_bytes());
    }
    let mut camera = Writer::new();
    model::write_camera(&mut camera, &frame.camera);
    hasher.update(&camera.into_bytes());
    for pass in &frame.post {
        hasher.update(&pass.shader.to_le_bytes());
        for value in &pass.values {
            hasher.update(&value.to_bits().to_le_bytes());
        }
    }
    *hasher.finalize().as_bytes()
}
