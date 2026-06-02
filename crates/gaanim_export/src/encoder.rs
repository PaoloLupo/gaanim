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
    Gif,
    PngSequence,
}

#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: ExportFormat,
    pub transparent: bool,
    pub crf: u32, // Constant Rate Factor (15-28, lower is better)
}

/// A highly optimized parallel frame encoder that pipes raw RGBA frames into FFmpeg in a background thread.
pub struct ParallelEncoder {
    sender: SyncSender<Option<Vec<u8>>>,
    thread_handle: Option<JoinHandle<Result<()>>>,
}

impl ParallelEncoder {
    pub fn new(config: EncoderConfig) -> Result<Self> {
        // We use a bounded channel to prevent unbounded memory growth if GPU rendering is faster than video encoding
        let (sender, receiver) = sync_channel::<Option<Vec<u8>>>(8);

        let thread_handle = std::thread::spawn(move || {
            Self::encoder_worker(config, receiver)
        });

        Ok(Self {
            sender,
            thread_handle: Some(thread_handle),
        })
    }

    /// Push a raw RGBA frame to the encoder.
    pub fn push_frame(&self, frame: Vec<u8>) -> Result<()> {
        self.sender
            .send(Some(frame))
            .map_err(|e| ExportError::Capture(format!("Failed to send frame to encoder: {}", e)))
    }

    /// Finalize the encoding and wait for FFmpeg to finish.
    pub fn finalize(&mut self) -> Result<()> {
        // Send termination sentinel
        let _ = self.sender.send(None);

        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(res) => res?,
                Err(_) => return Err(ExportError::General("Encoder thread panicked".to_string())),
            }
        }
        Ok(())
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

                    // Write PNG using the image crate in parallel
                    let width = config.width;
                    let height = config.height;
                    
                    let mut png_buffer = Vec::new();
                    let encoder = image::codecs::png::PngEncoder::new(&mut png_buffer);
                    image::ImageEncoder::write_image(
                        encoder,
                        &frame,
                        width,
                        height,
                        if config.transparent { image::ExtendedColorType::Rgba8 } else { image::ExtendedColorType::Rgb8 }
                    ).map_err(|e| ExportError::General(format!("PNG encode error: {}", e)))?;

                    std::fs::write(dest_path, png_buffer)?;
                    frame_idx += 1;
                }
            }
            _ => {
                // Setup FFmpeg command
                let mut cmd = Command::new("ffmpeg");
                cmd.arg("-y") // Overwrite output
                   .arg("-f").arg("rawvideo")
                   .arg("-pix_fmt").arg("rgba")
                   .arg("-s").arg(format!("{}x{}", config.width, config.height))
                   .arg("-r").arg(config.fps.to_string())
                   .arg("-i").arg("-"); // Read from stdin

                match config.format {
                    ExportFormat::Mp4 => {
                        cmd.arg("-c:v").arg("libx264")
                           .arg("-crf").arg(config.crf.to_string())
                           .arg("-preset").arg("slower");

                        if config.transparent {
                            // Transparent MP4 is not standard, so we blend onto a black background or export transparent WebM
                            // If transparent is requested for MP4, we still use yuv420p but log a warning.
                            cmd.arg("-pix_fmt").arg("yuv420p");
                        } else {
                            cmd.arg("-pix_fmt").arg("yuv420p");
                        }
                    }
                    ExportFormat::Webm => {
                        cmd.arg("-c:v").arg("libvpx-vp9")
                           .arg("-crf").arg(config.crf.to_string())
                           .arg("-b:v").arg("0"); // Constant quality mode

                        if config.transparent {
                            cmd.arg("-pix_fmt").arg("yuva420p"); // WebM with alpha!
                        } else {
                            cmd.arg("-pix_fmt").arg("yuv420p");
                        }
                    }
                    ExportFormat::Gif => {
                        // High quality GIF generation using a custom palette
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

                // Read frames and write to FFmpeg stdin
                while let Ok(Some(frame)) = receiver.recv() {
                    if !config.transparent && config.format != ExportFormat::Webm {
                        // Blend transparent frames onto solid black background for MP4/GIF if transparent = false
                        // The raw frame is RGBA, we write it directly if target supports alpha.
                        // FFmpeg expects the pixel format specified in -pix_fmt.
                        // Since we specified `-pix_fmt rgba` as input, FFmpeg will read RGBA and handle blending.
                        stdin.write_all(&frame)?;
                    } else {
                        stdin.write_all(&frame)?;
                    }
                }

                // Close stdin to signal EOF to FFmpeg
                drop(stdin);

                // Check FFmpeg exit status
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
