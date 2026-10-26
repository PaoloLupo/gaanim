//! Frame-by-frame animations played like a video: the frames of an
//! animated GIF, WebP or PNG, decoded once into memory, and numbered image
//! sequences, each file read when its frame is shown.
//!
//! Unlike a video file they need no FFmpeg, so they also play in the web
//! playground and record into bundles. A frame is chosen by time alone, so a
//! seek shows the same frame as playing up to it.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use gaanim_core::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use image::AnimationDecoder;

use crate::VideoMetadata;

/// Most bytes of RGBA pixels the decoded frames of one animated image may
/// take: a 1920×1080 animation fits about 64 frames.
pub const MAX_ANIMATED_BYTES: usize = 512 * 1024 * 1024;

/// How long a frame lasts when its file gives it 10 ms or less, as web
/// browsers show such GIFs.
const SHORT_FRAME: f64 = 0.1;

/// The frames of an animation and when each starts.
pub struct VideoFrames {
    pictures: Pictures,
    /// Source second at which each frame starts; the first starts at 0.
    starts: Vec<f64>,
    duration: f64,
    width: u32,
    height: u32,
}

enum Pictures {
    /// Decoded frames, shared by every drawable that shows them.
    Decoded(Vec<ImageData>),
    /// One image file per frame, read when the frame is shown.
    Files(Vec<PathBuf>),
}

impl std::fmt::Debug for VideoFrames {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VideoFrames")
            .field("frames", &self.starts.len())
            .field("duration", &self.duration)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("files", &matches!(self.pictures, Pictures::Files(_)))
            .finish()
    }
}

/// Why frames could not be read.
#[derive(Debug, thiserror::Error)]
pub enum FramesError {
    #[error("could not read '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not decode '{path}': {message}")]
    Decode { path: PathBuf, message: String },
    #[error("'{path}' has no frames")]
    Empty { path: PathBuf },
    #[error(
        "the frames of '{path}' take more than {} MB decoded; use a shorter or smaller animation",
        MAX_ANIMATED_BYTES / (1024 * 1024)
    )]
    TooLarge { path: PathBuf },
    #[error("an image sequence needs at least one frame")]
    NoFiles,
    #[error(
        "frame '{path}' is {width}×{height} pixels, but the first frame is {first_width}×{first_height}"
    )]
    SizeMismatch {
        path: PathBuf,
        width: u32,
        height: u32,
        first_width: u32,
        first_height: u32,
    },
    #[error("fps must be a finite positive number")]
    InvalidFps,
    #[error("the file name of '{pattern}' needs one %d, or %0Nd for numbers written with N digits")]
    InvalidPattern { pattern: PathBuf },
    #[error("no file matches '{pattern}'")]
    NoMatches { pattern: PathBuf },
}

/// The files that the numbered `pattern` names, by increasing number. Its
/// file name holds one `%d`, which a number of any length takes, or one
/// `%0Nd`, which numbers written with exactly N digits take:
/// `render/frame_%04d.png` names `render/frame_0001.png`, … in that order.
pub fn numbered_files(pattern: &Path) -> Result<Vec<PathBuf>, FramesError> {
    let invalid = || FramesError::InvalidPattern {
        pattern: pattern.to_path_buf(),
    };
    let name = pattern
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(invalid)?;
    let (prefix, rest) = name.split_once('%').ok_or_else(invalid)?;
    let (digits, suffix) = rest.split_once('d').ok_or_else(invalid)?;
    let width = match digits {
        "" => None,
        padded if padded.starts_with('0') && padded.len() > 1 => {
            Some(padded[1..].parse::<usize>().map_err(|_| invalid())?)
        }
        _ => return Err(invalid()),
    };
    if suffix.contains('%') {
        return Err(invalid());
    }
    let folder = match pattern.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder,
        _ => Path::new("."),
    };
    let entries = std::fs::read_dir(folder).map_err(|source| FramesError::Read {
        path: folder.to_path_buf(),
        source,
    })?;
    let mut found: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let number = file_name
                .to_str()?
                .strip_prefix(prefix)?
                .strip_suffix(suffix)?
                .to_owned();
            let matches = !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
                && width.is_none_or(|width| number.len() == width);
            if !matches || !entry.path().is_file() {
                return None;
            }
            Some((number.parse().ok()?, entry.path()))
        })
        .collect();
    if found.is_empty() {
        return Err(FramesError::NoMatches {
            pattern: pattern.to_path_buf(),
        });
    }
    found.sort();
    Ok(found.into_iter().map(|(_, path)| path).collect())
}

fn decode_error(path: &Path, error: impl std::fmt::Display) -> FramesError {
    FramesError::Decode {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn picture(image: image::RgbaImage) -> ImageData {
    let (width, height) = image.dimensions();
    ImageData {
        data: Blob::from(image.into_raw()),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

impl VideoFrames {
    /// Decode every frame of an animated GIF, WebP or PNG (APNG). Another
    /// image, or one that does not move, is one frame.
    pub fn decode(path: &Path) -> Result<Self, FramesError> {
        let bytes = std::fs::read(path).map_err(|source| FramesError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let format = image::guess_format(&bytes).map_err(|error| decode_error(path, error))?;
        let cursor = || Cursor::new(bytes.as_slice());
        let frames = match format {
            image::ImageFormat::Gif => image::codecs::gif::GifDecoder::new(cursor())
                .map_err(|error| decode_error(path, error))?
                .into_frames(),
            image::ImageFormat::WebP => {
                let mut decoder = image::codecs::webp::WebPDecoder::new(cursor())
                    .map_err(|error| decode_error(path, error))?;
                if decoder.has_animation() {
                    // Browsers and libwebp clear a disposed frame to
                    // transparent, whatever background the file names;
                    // without a color the decoder would leave it.
                    decoder
                        .set_background_color(image::Rgba([0, 0, 0, 0]))
                        .map_err(|error| decode_error(path, error))?;
                    decoder.into_frames()
                } else {
                    return Self::still(path, &bytes);
                }
            }
            image::ImageFormat::Png => {
                let decoder = image::codecs::png::PngDecoder::new(cursor())
                    .map_err(|error| decode_error(path, error))?;
                if decoder
                    .is_apng()
                    .map_err(|error| decode_error(path, error))?
                {
                    decoder
                        .apng()
                        .map_err(|error| decode_error(path, error))?
                        .into_frames()
                } else {
                    return Self::still(path, &bytes);
                }
            }
            _ => return Self::still(path, &bytes),
        };
        let mut pictures = Vec::new();
        let mut delays = Vec::new();
        let mut bytes_used = 0usize;
        for frame in frames {
            let frame = frame.map_err(|error| decode_error(path, error))?;
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            let delay = f64::from(numerator) / f64::from(denominator.max(1)) / 1000.0;
            let image = frame.into_buffer();
            bytes_used = bytes_used.saturating_add(image.as_raw().len());
            if bytes_used > MAX_ANIMATED_BYTES {
                return Err(FramesError::TooLarge {
                    path: path.to_path_buf(),
                });
            }
            pictures.push(picture(image));
            delays.push(if delay <= 0.010 { SHORT_FRAME } else { delay });
        }
        let Some(first) = pictures.first() else {
            return Err(FramesError::Empty {
                path: path.to_path_buf(),
            });
        };
        let (width, height) = (first.width, first.height);
        Ok(Self::timed(
            Pictures::Decoded(pictures),
            &delays,
            width,
            height,
        ))
    }

    fn still(path: &Path, bytes: &[u8]) -> Result<Self, FramesError> {
        let image = image::load_from_memory(bytes)
            .map_err(|error| decode_error(path, error))?
            .to_rgba8();
        let (width, height) = image.dimensions();
        Ok(Self::timed(
            Pictures::Decoded(vec![picture(image)]),
            &[SHORT_FRAME],
            width,
            height,
        ))
    }

    /// A sequence of image files shown `fps` frames per second. Every file
    /// must have the first one's size; each is read when it is shown.
    pub fn sequence(paths: Vec<PathBuf>, fps: f64) -> Result<Self, FramesError> {
        if !fps.is_finite() || fps <= 0.0 {
            return Err(FramesError::InvalidFps);
        }
        let mut first_size = None;
        for path in &paths {
            let (width, height) = image::image_dimensions(path).map_err(|error| match error {
                image::ImageError::IoError(source) => FramesError::Read {
                    path: path.clone(),
                    source,
                },
                error => decode_error(path, error),
            })?;
            match first_size {
                None => first_size = Some((width, height)),
                Some((first_width, first_height))
                    if (first_width, first_height) != (width, height) =>
                {
                    return Err(FramesError::SizeMismatch {
                        path: path.clone(),
                        width,
                        height,
                        first_width,
                        first_height,
                    });
                }
                Some(_) => {}
            }
        }
        let Some((width, height)) = first_size else {
            return Err(FramesError::NoFiles);
        };
        let delays = vec![1.0 / fps; paths.len()];
        Ok(Self::timed(Pictures::Files(paths), &delays, width, height))
    }

    fn timed(pictures: Pictures, delays: &[f64], width: u32, height: u32) -> Self {
        let mut starts = Vec::with_capacity(delays.len());
        let mut time = 0.0;
        for delay in delays {
            starts.push(time);
            time += delay;
        }
        Self {
            pictures,
            starts,
            duration: time,
            width,
            height,
        }
    }

    /// The number of frames.
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    /// Whether there are no frames; never true for frames that were read.
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// Seconds from the first frame's start to the last frame's end.
    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// Frames per second of the shortest frame. Frames are sampled at this
    /// rate, so no frame is ever skipped over.
    pub fn frame_rate(&self) -> f64 {
        let shortest = self
            .starts
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .chain([self.duration - self.starts.last().copied().unwrap_or(0.0)])
            .fold(f64::INFINITY, f64::min);
        1.0 / shortest
    }

    /// The video metadata of the animation: its size, length and rate,
    /// without audio.
    pub fn metadata(&self) -> VideoMetadata {
        VideoMetadata {
            width: self.width,
            height: self.height,
            duration: self.duration,
            fps: self.frame_rate(),
            has_audio: false,
        }
    }

    /// The frame shown at `time` source seconds: the first before the
    /// start, the last from its start on. A time a rounding error short of
    /// a frame's start already shows that frame, as the last frame's start
    /// is computed by subtracting its length from the duration.
    pub fn index_at(&self, time: f64) -> usize {
        self.starts
            .partition_point(|start| *start <= time + 1e-9)
            .saturating_sub(1)
    }

    /// The file frame `index` is read from, for an image sequence.
    pub fn file(&self, index: usize) -> Option<&Path> {
        match &self.pictures {
            Pictures::Files(paths) => paths.get(index).map(PathBuf::as_path),
            Pictures::Decoded(_) => None,
        }
    }

    /// The picture of frame `index` (the last one past the end), reading
    /// its file for an image sequence.
    pub fn picture(&self, index: usize) -> Result<ImageData, FramesError> {
        let index = index.min(self.len().saturating_sub(1));
        match &self.pictures {
            Pictures::Decoded(pictures) => Ok(pictures[index].clone()),
            Pictures::Files(paths) => {
                let path = &paths[index];
                let image = image::open(path).map_err(|error| decode_error(path, error))?;
                Ok(picture(image.to_rgba8()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, Rgba, RgbaImage};

    fn solid(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba(color))
    }

    fn write_gif(path: &Path, frames: &[([u8; 4], u32)]) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = GifEncoder::new(file);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for (color, delay_ms) in frames {
            encoder
                .encode_frame(Frame::from_parts(
                    solid(4, 3, *color),
                    0,
                    0,
                    Delay::from_numer_denom_ms(*delay_ms, 1),
                ))
                .unwrap();
        }
    }

    #[test]
    fn a_gif_decodes_every_frame_with_its_delay() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("blink.gif");
        write_gif(
            &path,
            &[
                ([255, 0, 0, 255], 100),
                ([0, 255, 0, 255], 200),
                ([0, 0, 255, 255], 0),
            ],
        );
        let frames = VideoFrames::decode(&path).unwrap();
        assert_eq!(frames.len(), 3);
        assert!(
            (frames.duration() - 0.4).abs() < 1e-9,
            "{}",
            frames.duration()
        );
        assert!((frames.frame_rate() - 10.0).abs() < 1e-9);
        let metadata = frames.metadata();
        assert_eq!((metadata.width, metadata.height), (4, 3));
        assert!(!metadata.has_audio);
        assert_eq!(frames.index_at(-1.0), 0);
        assert_eq!(frames.index_at(0.05), 0);
        assert_eq!(frames.index_at(0.1), 1);
        assert_eq!(frames.index_at(0.29), 1);
        assert_eq!(frames.index_at(0.31), 2);
        assert_eq!(frames.index_at(9.0), 2);
        let green = frames.picture(1).unwrap();
        assert_eq!(&green.data.data()[..4], &[0, 255, 0, 255]);
        assert!(frames.file(1).is_none());
    }

    #[test]
    fn webp_frames_are_disposed_to_transparent() {
        // Twenty frames of a dot circling another, made by libwebp.
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("orbit.webp");
        std::fs::write(
            &path,
            include_bytes!("../../../examples/media/assets/orbit.webp"),
        )
        .unwrap();
        let frames = VideoFrames::decode(&path).unwrap();
        assert_eq!(frames.len(), 20);
        let opaque = |index: usize| {
            let picture = frames.picture(index).unwrap();
            picture.data.data().chunks_exact(4).filter(|pixel| pixel[3] > 128).count()
        };
        // Each frame shows the two dots alone: one that kept the dots of
        // the frames before would cover more of the canvas.
        let first = opaque(0);
        for index in 1..frames.len() {
            let count = opaque(index);
            assert!(count < first + first / 10, "frame {index}: {first} → {count}");
        }
    }

    #[test]
    fn a_still_image_is_one_frame() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("still.png");
        solid(2, 2, [9, 9, 9, 255]).save(&path).unwrap();
        let frames = VideoFrames::decode(&path).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames.index_at(5.0), 0);
        assert!(frames.duration() > 0.0);
    }

    #[test]
    fn a_sequence_reads_each_file_when_shown() {
        let folder = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..3)
            .map(|index| {
                let path = folder.path().join(format!("frame_{index:04}.png"));
                solid(3, 2, [index as u8 * 50, 0, 0, 255])
                    .save(&path)
                    .unwrap();
                path
            })
            .collect();
        let frames = VideoFrames::sequence(paths.clone(), 12.0).unwrap();
        assert_eq!(frames.len(), 3);
        assert!((frames.duration() - 0.25).abs() < 1e-9);
        assert!((frames.frame_rate() - 12.0).abs() < 1e-9);
        assert_eq!(frames.index_at(1.0 / 12.0), 1);
        assert_eq!(frames.file(2), Some(paths[2].as_path()));
        assert_eq!(frames.picture(2).unwrap().data.data()[0], 100);

        assert!(matches!(
            VideoFrames::sequence(paths.clone(), 0.0),
            Err(FramesError::InvalidFps)
        ));
        assert!(matches!(
            VideoFrames::sequence(Vec::new(), 24.0),
            Err(FramesError::NoFiles)
        ));
        let odd = folder.path().join("odd.png");
        solid(5, 5, [0, 0, 0, 255]).save(&odd).unwrap();
        assert!(matches!(
            VideoFrames::sequence(vec![paths[0].clone(), odd], 24.0),
            Err(FramesError::SizeMismatch { width: 5, .. })
        ));
        assert!(matches!(
            VideoFrames::sequence(vec![folder.path().join("missing.png")], 24.0),
            Err(FramesError::Read { .. })
        ));
    }

    #[test]
    fn numbered_patterns_list_their_files_in_order() {
        let folder = tempfile::tempdir().unwrap();
        for name in [
            "frame_0010.png",
            "frame_0002.png",
            "frame_0001.png",
            "frame_01.png",
            "frame_0003.jpg",
            "other_0004.png",
        ] {
            std::fs::write(folder.path().join(name), b"").unwrap();
        }
        let names = |pattern: &str| -> Vec<String> {
            numbered_files(&folder.path().join(pattern))
                .unwrap()
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            names("frame_%04d.png"),
            ["frame_0001.png", "frame_0002.png", "frame_0010.png"]
        );
        assert_eq!(
            names("frame_%d.png"),
            [
                "frame_0001.png",
                "frame_01.png",
                "frame_0002.png",
                "frame_0010.png"
            ]
        );
        for pattern in [
            "frame.png",
            "frame_%4d.png",
            "frame_%d_%d.png",
            "frame_%x.png",
        ] {
            assert!(
                matches!(
                    numbered_files(&folder.path().join(pattern)),
                    Err(FramesError::InvalidPattern { .. })
                ),
                "{pattern}"
            );
        }
        assert!(matches!(
            numbered_files(&folder.path().join("none_%03d.png")),
            Err(FramesError::NoMatches { .. })
        ));
    }

    #[test]
    fn unreadable_files_fail_with_their_path() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("broken.gif");
        std::fs::write(&path, b"GIF89a not really").unwrap();
        assert!(matches!(
            VideoFrames::decode(&path),
            Err(FramesError::Decode { .. })
        ));
        assert!(matches!(
            VideoFrames::decode(&folder.path().join("none.gif")),
            Err(FramesError::Read { .. })
        ));
    }
}
