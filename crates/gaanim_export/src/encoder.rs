use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread::JoinHandle;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExportError {
    #[error("FFmpeg error: {0}")]
    FFmpeg(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Frame capture error: {0}")]
    Capture(String),
    #[error("Crate error: {0}")]
    General(String),
}

pub type Result<T> = std::result::Result<T, ExportError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Mp4,
    Webm,
    Webp,
    Gif,
    PngSequence,
}

/// Encoding speed tier sent from the config layer.
/// 0 = fast (Draft), 1 = balanced (Standard), 2 = best (Production).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingSpeed {
    Fast = 0,
    Balanced = 1,
    Best = 2,
}

impl EncodingSpeed {
    pub fn from_u8(v: u8) -> Self {
        match v {
            2 => Self::Best,
            1 => Self::Balanced,
            _ => Self::Fast,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: ExportFormat,
    pub transparent: bool,
    pub crf: u32,
    pub encoding_speed: EncodingSpeed,
}

/// A highly optimized parallel frame encoder that pipes raw RGBA frames into FFmpeg in a background thread.
pub struct ParallelEncoder {
    sender: SyncSender<Option<Vec<u8>>>,
    thread_handle: Option<JoinHandle<Result<()>>>,
}

/// Compute an adaptive channel depth: reserve at least 4 slots, up to 8,
/// scaling inversely with frame size to keep memory bounded.
fn adaptive_buffer_depth(width: u32, height: u32) -> usize {
    let pixels = width as u64 * height as u64 * 4;
    let per_frame_mb = pixels / (1024 * 1024);
    if per_frame_mb >= 32 {
        4
    } else if per_frame_mb >= 16 {
        6
    } else {
        8
    }
}

impl ParallelEncoder {
    pub fn new(config: EncoderConfig) -> Result<Self> {
        let depth = adaptive_buffer_depth(config.width, config.height);
        let (sender, receiver) = sync_channel::<Option<Vec<u8>>>(depth);

        let thread_handle = std::thread::spawn(move || {
            Self::encoder_worker(config, receiver)
        });

        Ok(Self {
            sender,
            thread_handle: Some(thread_handle),
        })
    }

    pub fn push_frame(&self, frame: Vec<u8>) -> Result<()> {
        self.sender
            .send(Some(frame))
            .map_err(|e| ExportError::Capture(format!("Failed to send frame to encoder: {}", e)))
    }

    pub fn finalize(&mut self) -> Result<()> {
        let _ = self.sender.send(None);

        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(res) => res?,
                Err(_) => return Err(ExportError::General("Encoder thread panicked".to_string())),
            }
        }
        Ok(())
    }

    fn x264_preset(speed: EncodingSpeed) -> &'static str {
        match speed {
            EncodingSpeed::Fast => "fast",
            EncodingSpeed::Balanced => "medium",
            EncodingSpeed::Best => "slower",
        }
    }

    fn webp_quality(crf: u32) -> u32 {
        ((100_f64 * (1.0 - (crf as f64 - 14.0) / 14.0)) as u32).clamp(10, 100)
    }

    fn webp_compression(speed: EncodingSpeed) -> u32 {
        match speed {
            EncodingSpeed::Fast => 2,
            EncodingSpeed::Balanced => 4,
            EncodingSpeed::Best => 6,
        }
    }

    fn encoder_worker(config: EncoderConfig, receiver: Receiver<Option<Vec<u8>>>) -> Result<()> {
        match config.format {
            ExportFormat::PngSequence => {
                let base_path = std::path::Path::new(&config.output_path);
                if let Some(parent) = base_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut frame_idx = 0;
                while let Ok(Some(frame)) = receiver.recv() {
                    let filename = format!(
                        "{}_{:05}.png",
                        base_path.file_stem().and_then(|s| s.to_str()).unwrap_or("frame"),
                        frame_idx
                    );
                    let dest_path = base_path.parent().unwrap_or(std::path::Path::new("")).join(filename);

                    let width = config.width;
                    let height = config.height;
                    let color_type = if config.transparent {
                        image::ExtendedColorType::Rgba8
                    } else {
                        image::ExtendedColorType::Rgb8
                    };

                    let mut png_buffer = Vec::new();
                    let encoder = image::codecs::png::PngEncoder::new(&mut png_buffer);
                    image::ImageEncoder::write_image(encoder, &frame, width, height, color_type)
                        .map_err(|e| ExportError::General(format!("PNG encode error: {}", e)))?;

                    std::fs::write(dest_path, png_buffer)?;
                    frame_idx += 1;
                }
            }
            _ => {
                let mut cmd = Command::new("ffmpeg");
                cmd.arg("-y")
                   .arg("-f").arg("rawvideo")
                   .arg("-pix_fmt").arg("rgba")
                   .arg("-s").arg(format!("{}x{}", config.width, config.height))
                   .arg("-r").arg(config.fps.to_string())
                   .arg("-i").arg("-");

                match config.format {
                    ExportFormat::Mp4 => {
                        cmd.arg("-c:v").arg("libx264")
                           .arg("-crf").arg(config.crf.to_string())
                           .arg("-preset").arg(Self::x264_preset(config.encoding_speed))
                           .arg("-threads").arg("0");

                        cmd.arg("-pix_fmt").arg("yuv420p");
                    }
                    ExportFormat::Webm => {
                        cmd.arg("-c:v").arg("libvpx-vp9")
                           .arg("-crf").arg(config.crf.to_string())
                           .arg("-b:v").arg("0")
                           .arg("-threads").arg("0");

                        if config.transparent {
                            cmd.arg("-pix_fmt").arg("yuva420p");
                        } else {
                            cmd.arg("-pix_fmt").arg("yuv420p");
                        }
                    }
                    ExportFormat::Webp => {
                        let quality = Self::webp_quality(config.crf);
                        let compression = Self::webp_compression(config.encoding_speed);

                        cmd.arg("-c:v").arg("libwebp")
                           .arg("-lossless").arg("0")
                           .arg("-compression_level").arg(compression.to_string())
                           .arg("-quality").arg(quality.to_string())
                           .arg("-loop").arg("0")
                           .arg("-threads").arg("0");

                        if config.transparent {
                            cmd.arg("-pix_fmt").arg("yuva420p");
                        } else {
                            cmd.arg("-pix_fmt").arg("yuv420p");
                        }
                    }
                    ExportFormat::Gif => {
                        cmd.arg("-filter_complex")
                           .arg("[0:v] split [a][b];[a] palettegen=stats_mode=single [p];[b][p] paletteuse=new=1");
                    }
                    _ => unreachable!(),
                }

                cmd.arg(&config.output_path);

                cmd.stdin(Stdio::piped())
                   .stdout(Stdio::null())
                   .stderr(Stdio::piped());

                let mut child = cmd.spawn().map_err(|e| {
                    ExportError::FFmpeg(format!(
                        "Failed to spawn FFmpeg. Make sure it is installed and in your PATH. Error: {}",
                        e
                    ))
                })?;

                let mut stdin = child.stdin.take().ok_or_else(|| {
                    ExportError::FFmpeg("Failed to open stdin pipe to FFmpeg".to_string())
                })?;

                let stderr = child.stderr.take().ok_or_else(|| {
                    ExportError::FFmpeg("Failed to open stderr pipe to FFmpeg".to_string())
                })?;

                while let Ok(Some(frame)) = receiver.recv() {
                    stdin.write_all(&frame)?;
                }

                drop(stdin);

                let status = child.wait()?;
                if !status.success() {
                    let mut stderr_content = String::new();
                    use std::io::Read;
                    let mut stderr_reader = std::io::BufReader::new(stderr);
                    let _ = stderr_reader.read_to_string(&mut stderr_content);
                    return Err(ExportError::FFmpeg(format!(
                        "FFmpeg exited with non-zero status. Stderr:\n{}",
                        stderr_content
                    )));
                }
            }
        }

        Ok(())
    }
}
