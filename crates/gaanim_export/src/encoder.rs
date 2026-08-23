use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::config::AudioTrack;

const FFMPEG_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const FFMPEG_PROBE_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Error, Debug)]
pub enum ExportError {
    #[error("FFmpeg error: {0}")]
    FFmpeg(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Frame capture error: {0}")]
    Capture(String),
    #[error(transparent)]
    Gpu(#[from] crate::gpu::GpuContextError),
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

/// Hardware / software video encoder selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoEncoder {
    /// Probe safe automatic candidates and fall back to libx264 when none works.
    #[default]
    Auto,
    /// CPU libx264 — always available
    Libx264,
    /// NVIDIA NVENC
    H264Nvenc,
    /// AMD AMF
    H264Amf,
    /// Intel Quick Sync
    H264Qsv,
    /// Linux VAAPI. Explicit-only because driver failures can reset the GPU.
    H264Vaapi,
}

impl VideoEncoder {
    pub const ARG_VALUES: &'static [&'static str] =
        &["auto", "libx264", "nvenc", "amf", "qsv", "vaapi"];

    pub fn arg_name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Libx264 => "libx264",
            Self::H264Nvenc => "nvenc",
            Self::H264Amf => "amf",
            Self::H264Qsv => "qsv",
            Self::H264Vaapi => "vaapi",
        }
    }

    pub fn parse_arg(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "libx264" => Some(Self::Libx264),
            "nvenc" => Some(Self::H264Nvenc),
            "amf" => Some(Self::H264Amf),
            "qsv" => Some(Self::H264Qsv),
            "vaapi" => Some(Self::H264Vaapi),
            _ => None,
        }
    }

    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Libx264 => "libx264",
            Self::H264Nvenc => "h264_nvenc",
            Self::H264Amf => "h264_amf",
            Self::H264Qsv => "h264_qsv",
            Self::H264Vaapi => "h264_vaapi",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic (hardware preferred)",
            Self::Libx264 => "CPU (libx264)",
            Self::H264Nvenc => "NVIDIA (NVENC)",
            Self::H264Amf => "AMD (AMF)",
            Self::H264Qsv => "Intel (QSV)",
            Self::H264Vaapi => "VAAPI (Linux)",
        }
    }
}

/// Detect which hardware H.264 encoders are available via ffmpeg.
pub fn detect_available_encoders() -> Vec<VideoEncoder> {
    let mut encoders = vec![VideoEncoder::Libx264];

    let Ok(mut child) = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return encoders;
    };
    let stdout_thread = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            bytes
        })
    });
    let status = wait_for_child(&mut child, FFMPEG_PROBE_TIMEOUT);
    if !matches!(&status, Ok(Some(status)) if status.success()) {
        let _ = child.kill();
        std::thread::spawn(move || {
            let _ = child.wait();
            if let Some(stdout_thread) = stdout_thread {
                let _ = stdout_thread.join();
            }
        });
        return encoders;
    }
    let bytes = stdout_thread
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    if matches!(&status, Ok(Some(status)) if status.success()) {
        let text = String::from_utf8_lossy(&bytes);
        if text.contains("h264_nvenc") {
            encoders.push(VideoEncoder::H264Nvenc);
        }
        if text.contains("h264_amf") {
            encoders.push(VideoEncoder::H264Amf);
        }
        if text.contains("h264_qsv") {
            encoders.push(VideoEncoder::H264Qsv);
        }
        if text.contains("h264_vaapi") {
            encoders.push(VideoEncoder::H264Vaapi);
        }
    }

    encoders
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    let started_at = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if started_at.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(FFMPEG_PROBE_POLL_INTERVAL);
    }
}

/// Pick the best available encoder preferring hardware acceleration.
///
/// Probes each candidate with a 1-frame test encode to verify it works
/// with the piped-rawvideo input we use (some encoders need driver-specific
/// setup or proprietary drivers, e.g. AMF needs AMDGPU-PRO on Linux).
pub fn detect_best_encoder() -> VideoEncoder {
    let available = detect_available_encoders();
    select_best_encoder(&available, probe_encoder)
}

fn select_best_encoder(
    available: &[VideoEncoder],
    mut probe: impl FnMut(VideoEncoder) -> bool,
) -> VideoEncoder {
    // Do not auto-probe VAAPI: a one-frame success does not prove that a
    // sustained encode is safe, and a VCE/driver failure can hang the kernel.
    for candidate in [
        VideoEncoder::H264Nvenc,
        VideoEncoder::H264Amf,
        VideoEncoder::H264Qsv,
    ] {
        if available.contains(&candidate) && probe(candidate) {
            return candidate;
        }
    }
    VideoEncoder::Libx264
}

pub(crate) fn resolve_video_encoder(format: ExportFormat, requested: VideoEncoder) -> VideoEncoder {
    match (format, requested) {
        (ExportFormat::Mp4, VideoEncoder::Auto) => detect_best_encoder(),
        (_, VideoEncoder::Auto) => VideoEncoder::Libx264,
        (_, requested) => requested,
    }
}

fn probe_encoder(encoder: VideoEncoder) -> bool {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");

    if matches!(encoder, VideoEncoder::H264Vaapi) {
        cmd.arg("-vaapi_device").arg("/dev/dri/renderD128");
    }

    cmd.arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-s")
        // NVENC rejects 128x128 on some drivers even though it supports all
        // resolutions used by the editor. Probe with a normal video size.
        .arg("640x360")
        .arg("-r")
        .arg("1")
        .arg("-i")
        .arg("-")
        .arg("-frames:v")
        .arg("1");

    match encoder {
        VideoEncoder::Auto => return false,
        VideoEncoder::Libx264 => {
            cmd.arg("-c:v")
                .arg("libx264")
                .arg("-preset")
                .arg("ultrafast");
        }
        VideoEncoder::H264Nvenc => {
            cmd.arg("-c:v").arg("h264_nvenc").arg("-preset").arg("p1");
        }
        VideoEncoder::H264Amf => {
            cmd.arg("-vf")
                .arg("format=yuv420p")
                .arg("-c:v")
                .arg("h264_amf")
                .arg("-quality")
                .arg("speed")
                .arg("-rc")
                .arg("vbr_latency");
        }
        VideoEncoder::H264Qsv => {
            cmd.arg("-c:v").arg("h264_qsv");
        }
        VideoEncoder::H264Vaapi => {
            cmd.arg("-vf")
                .arg("format=nv12,hwupload")
                .arg("-c:v")
                .arg("h264_vaapi");
        }
    }

    if !matches!(encoder, VideoEncoder::H264Vaapi) {
        cmd.arg("-pix_fmt").arg("yuv420p");
    }
    cmd.arg("-f")
        .arg("null")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Ok(mut child) = cmd.spawn() {
        // Feed one empty RGBA frame (640x360x4 bytes).
        let writer = child.stdin.take().map(|mut stdin| {
            let blank = vec![0u8; 640 * 360 * 4];
            std::thread::spawn(move || stdin.write_all(&blank))
        });
        let status = wait_for_child(&mut child, FFMPEG_PROBE_TIMEOUT);
        let succeeded = matches!(&status, Ok(Some(status)) if status.success());
        if matches!(status, Ok(Some(_))) {
            if let Some(writer) = writer {
                let _ = writer.join();
            }
        } else {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
                if let Some(writer) = writer {
                    let _ = writer.join();
                }
            });
        }
        succeeded
    } else {
        false
    }
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
    pub video_encoder: VideoEncoder,
    /// Tracks use scene-time coordinates; these values place them in the
    /// exported range before FFmpeg mixes them.
    pub audio_tracks: Vec<AudioTrack>,
    pub render_start: f64,
    pub render_duration: f64,
}

/// A highly optimized parallel frame encoder that pipes raw RGBA frames into FFmpeg in a background thread.
pub struct ParallelEncoder {
    sender: SyncSender<Option<Vec<u8>>>,
    thread_handle: Option<JoinHandle<Result<Duration>>>,
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

fn png_pixels(
    frame: Vec<u8>,
    width: u32,
    height: u32,
    transparent: bool,
) -> Result<(Vec<u8>, image::ExtendedColorType)> {
    let expected_rgba = width as usize * height as usize * 4;
    if frame.len() != expected_rgba {
        return Err(ExportError::Capture(format!(
            "PNG frame contains {} bytes; expected {expected_rgba} RGBA bytes for {width}x{height}",
            frame.len()
        )));
    }
    if transparent {
        return Ok((frame, image::ExtendedColorType::Rgba8));
    }

    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for pixel in frame.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    Ok((rgb, image::ExtendedColorType::Rgb8))
}

fn validate_transparency(format: ExportFormat, transparent: bool) -> Result<()> {
    if transparent
        && !matches!(
            format,
            ExportFormat::Webm | ExportFormat::Webp | ExportFormat::PngSequence
        )
    {
        return Err(ExportError::General(
            "transparent export requires WebM, WebP, or PNG output".to_string(),
        ));
    }
    Ok(())
}

impl ParallelEncoder {
    pub fn new(mut config: EncoderConfig) -> Result<Self> {
        validate_transparency(config.format, config.transparent)?;
        config.video_encoder = resolve_video_encoder(config.format, config.video_encoder);
        let depth = adaptive_buffer_depth(config.width, config.height);
        let (sender, receiver) = sync_channel::<Option<Vec<u8>>>(depth);

        let thread_handle = std::thread::spawn(move || Self::encoder_worker(config, receiver));

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
        self.finalize_with_timings().map(|_| ())
    }

    /// Finish the encoder and return time spent actively writing/encoding frames.
    /// This excludes time waiting for frames and the final drain while joining.
    pub(crate) fn finalize_with_timings(&mut self) -> Result<Duration> {
        let _ = self.sender.send(None);

        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(res) => return res,
                Err(_) => return Err(ExportError::General("Encoder thread panicked".to_string())),
            }
        }
        Ok(Duration::ZERO)
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

    fn audio_filter(track: &AudioTrack, input_index: usize, render_start: f64) -> String {
        let relative_start = track.start_time - render_start;
        let output_trim = (-relative_start).max(0.0);
        let mut filter = format!("[{input_index}:a]aresample=48000");
        if track.source_offset > 0.0 || track.source_duration.is_some() {
            filter.push_str(&format!(",atrim=start={:.6}", track.source_offset));
            if let Some(duration) = track.source_duration {
                filter.push_str(&format!(":duration={duration:.6}"));
            }
            filter.push_str(",asetpts=PTS-STARTPTS");
        }
        if track.looping {
            let samples = track
                .source_duration
                .map(|duration| (duration * 48_000.0).round().max(1.0) as u64)
                .unwrap_or(2_147_483_647);
            filter.push_str(&format!(",aloop=loop=-1:size={samples}"));
        }
        let mut tempo = track.speed;
        while tempo < 0.5 {
            filter.push_str(",atempo=0.5");
            tempo /= 0.5;
        }
        while tempo > 2.0 {
            filter.push_str(",atempo=2.0");
            tempo /= 2.0;
        }
        if (tempo - 1.0).abs() > 1e-9 {
            filter.push_str(&format!(",atempo={tempo:.9}"));
        }
        if output_trim > 0.0 || track.duration.is_some() {
            filter.push_str(&format!(",atrim=start={output_trim:.6}"));
            if let Some(duration) = track.duration {
                let remaining = (duration - output_trim).max(0.0);
                filter.push_str(&format!(":duration={remaining:.6}"));
            }
            filter.push_str(",asetpts=PTS-STARTPTS");
        }
        filter.push_str(&format!(",volume={:.6}", track.volume));
        if track.fade_in > 0.0 {
            filter.push_str(&format!(",afade=t=in:st=0:d={:.6}", track.fade_in));
        }
        if track.fade_out > 0.0 {
            let duration = track.duration.expect("validated audio fade-out duration");
            filter.push_str(&format!(
                ",afade=t=out:st={:.6}:d={:.6}",
                (duration - track.fade_out).max(0.0),
                track.fade_out
            ));
        }
        if relative_start > 0.0 {
            let delay_ms = (relative_start * 1000.0).round().max(0.0) as u64;
            filter.push_str(&format!(",adelay={delay_ms}:all=1"));
        }
        filter
    }

    fn add_audio_inputs(cmd: &mut Command, config: &EncoderConfig) -> Result<Option<String>> {
        if config.audio_tracks.is_empty() {
            return Ok(None);
        }
        if !matches!(config.format, ExportFormat::Mp4 | ExportFormat::Webm) {
            return Err(ExportError::General(
                "audio tracks can only be exported to MP4 or WebM".to_string(),
            ));
        }
        for track in &config.audio_tracks {
            cmd.arg("-i").arg(&track.path);
        }
        let filters = config
            .audio_tracks
            .iter()
            .enumerate()
            .map(|(index, track)| {
                format!(
                    "{}[audio_{index}]",
                    Self::audio_filter(track, index + 1, config.render_start)
                )
            })
            .collect::<Vec<_>>();
        let inputs = (0..config.audio_tracks.len())
            .map(|index| format!("[audio_{index}]"))
            .collect::<String>();
        let mix = format!(
            "{inputs}amix=inputs={}:duration=longest:normalize=0,atrim=duration={:.6}[audio_out]",
            config.audio_tracks.len(),
            config.render_duration
        );
        cmd.arg("-filter_complex")
            .arg(filters.join(";") + ";" + &mix);
        Ok(Some("[audio_out]".to_string()))
    }

    fn encoder_worker(
        config: EncoderConfig,
        receiver: Receiver<Option<Vec<u8>>>,
    ) -> Result<Duration> {
        let mut encode_time = Duration::ZERO;
        match config.format {
            ExportFormat::PngSequence => {
                let base_path = std::path::Path::new(&config.output_path);
                if let Some(parent) = base_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut frame_idx = 0;
                while let Ok(Some(frame)) = receiver.recv() {
                    let encode_started_at = Instant::now();
                    let filename = format!(
                        "{}_{:05}.png",
                        base_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("frame"),
                        frame_idx
                    );
                    let dest_path = base_path
                        .parent()
                        .unwrap_or(std::path::Path::new(""))
                        .join(filename);

                    let width = config.width;
                    let height = config.height;
                    let (pixels, color_type) =
                        png_pixels(frame, width, height, config.transparent)?;

                    let mut png_buffer = Vec::new();
                    let encoder = image::codecs::png::PngEncoder::new(&mut png_buffer);
                    image::ImageEncoder::write_image(encoder, &pixels, width, height, color_type)
                        .map_err(|e| ExportError::General(format!("PNG encode error: {}", e)))?;

                    std::fs::write(dest_path, png_buffer)?;
                    encode_time += encode_started_at.elapsed();
                    frame_idx += 1;
                }
            }
            _ => {
                let mut cmd = Command::new("ffmpeg");
                cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-nostats"]);

                if matches!(config.video_encoder, VideoEncoder::H264Vaapi) {
                    cmd.arg("-vaapi_device").arg("/dev/dri/renderD128");
                }

                cmd.arg("-f")
                    .arg("rawvideo")
                    .arg("-pix_fmt")
                    .arg("rgba")
                    .arg("-s")
                    .arg(format!("{}x{}", config.width, config.height))
                    .arg("-r")
                    .arg(config.fps.to_string())
                    .arg("-i")
                    .arg("-");

                let audio_output = Self::add_audio_inputs(&mut cmd, &config)?;

                match config.format {
                    ExportFormat::Mp4 => {
                        match config.video_encoder {
                            VideoEncoder::Auto => {
                                unreachable!("automatic video encoder must be resolved before use")
                            }
                            VideoEncoder::Libx264 => {
                                cmd.arg("-c:v")
                                    .arg("libx264")
                                    .arg("-crf")
                                    .arg(config.crf.to_string())
                                    .arg("-preset")
                                    .arg(Self::x264_preset(config.encoding_speed))
                                    .arg("-threads")
                                    .arg("0");
                            }
                            VideoEncoder::H264Nvenc => {
                                let p = match config.encoding_speed {
                                    EncodingSpeed::Fast => "p1",
                                    EncodingSpeed::Balanced => "p4",
                                    EncodingSpeed::Best => "p7",
                                };
                                cmd.arg("-c:v")
                                    .arg("h264_nvenc")
                                    .arg("-preset")
                                    .arg(p)
                                    .arg("-rc")
                                    .arg("vbr")
                                    .arg("-cq")
                                    .arg(config.crf.to_string());
                            }
                            VideoEncoder::H264Amf => {
                                let quality = match config.encoding_speed {
                                    EncodingSpeed::Fast => "speed",
                                    EncodingSpeed::Balanced => "balanced",
                                    EncodingSpeed::Best => "quality",
                                };
                                cmd.arg("-vf")
                                    .arg("format=yuv420p")
                                    .arg("-c:v")
                                    .arg("h264_amf")
                                    .arg("-quality")
                                    .arg(quality)
                                    .arg("-rc")
                                    .arg("vbr_latency");
                            }
                            VideoEncoder::H264Qsv => {
                                cmd.arg("-c:v")
                                    .arg("h264_qsv")
                                    .arg("-global_quality")
                                    .arg(config.crf.to_string());
                            }
                            VideoEncoder::H264Vaapi => {
                                cmd.arg("-vf")
                                    .arg("format=nv12,hwupload")
                                    .arg("-c:v")
                                    .arg("h264_vaapi")
                                    .arg("-qp")
                                    .arg(config.crf.to_string());
                            }
                        }

                        if !matches!(config.video_encoder, VideoEncoder::H264Vaapi) {
                            cmd.arg("-pix_fmt").arg("yuv420p");
                        }
                    }
                    ExportFormat::Webm => {
                        cmd.arg("-c:v")
                            .arg("libvpx-vp9")
                            .arg("-crf")
                            .arg(config.crf.to_string())
                            .arg("-b:v")
                            .arg("0")
                            .arg("-threads")
                            .arg("0");

                        if config.transparent {
                            cmd.arg("-pix_fmt").arg("yuva420p");
                        } else {
                            cmd.arg("-pix_fmt").arg("yuv420p");
                        }
                    }
                    ExportFormat::Webp => {
                        let quality = Self::webp_quality(config.crf);
                        let compression = Self::webp_compression(config.encoding_speed);

                        cmd.arg("-c:v")
                            .arg("libwebp")
                            .arg("-lossless")
                            .arg("0")
                            .arg("-compression_level")
                            .arg(compression.to_string())
                            .arg("-quality")
                            .arg(quality.to_string())
                            .arg("-loop")
                            .arg("0")
                            .arg("-threads")
                            .arg("0");

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

                if let Some(audio_output) = audio_output {
                    cmd.arg("-map").arg("0:v:0").arg("-map").arg(audio_output);
                    match config.format {
                        ExportFormat::Mp4 => {
                            cmd.arg("-c:a").arg("aac").arg("-b:a").arg("192k");
                        }
                        ExportFormat::Webm => {
                            cmd.arg("-c:a").arg("libopus").arg("-b:a").arg("128k");
                        }
                        _ => unreachable!("validated audio export format"),
                    }
                    cmd.arg("-shortest");
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

                let mut write_error = None;
                while let Ok(Some(frame)) = receiver.recv() {
                    let encode_started_at = Instant::now();
                    if let Err(error) = stdin.write_all(&frame) {
                        write_error = Some(error);
                        break;
                    }
                    encode_time += encode_started_at.elapsed();
                }

                drop(stdin);

                let status = child.wait()?;
                let mut stderr_content = String::new();
                let mut stderr_reader = std::io::BufReader::new(stderr);
                let _ = stderr_reader.read_to_string(&mut stderr_content);
                let stderr_content = stderr_content.trim();
                if let Some(error) = write_error {
                    return Err(ExportError::FFmpeg(format!(
                        "FFmpeg stopped accepting frames ({status}): {error}. Stderr:\n{}",
                        if stderr_content.is_empty() {
                            "(no stderr output)"
                        } else {
                            stderr_content
                        }
                    )));
                }
                if !status.success() {
                    return Err(ExportError::FFmpeg(format!(
                        "FFmpeg exited with {status}. Stderr:\n{}",
                        if stderr_content.is_empty() {
                            "(no stderr output)"
                        } else {
                            stderr_content
                        }
                    )));
                }
            }
        }

        Ok(encode_time)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExportFormat, VideoEncoder, png_pixels, select_best_encoder, validate_transparency,
    };

    #[test]
    fn video_encoder_argument_names_round_trip() {
        let encoders = [
            VideoEncoder::Auto,
            VideoEncoder::Libx264,
            VideoEncoder::H264Nvenc,
            VideoEncoder::H264Amf,
            VideoEncoder::H264Qsv,
            VideoEncoder::H264Vaapi,
        ];

        assert_eq!(VideoEncoder::ARG_VALUES.len(), encoders.len());
        for encoder in encoders {
            assert_eq!(VideoEncoder::parse_arg(encoder.arg_name()), Some(encoder));
        }
    }

    #[test]
    fn automatic_selection_uses_the_first_working_hardware_encoder_or_software() {
        let available = [
            VideoEncoder::Libx264,
            VideoEncoder::H264Nvenc,
            VideoEncoder::H264Qsv,
            VideoEncoder::H264Vaapi,
        ];

        assert_eq!(
            select_best_encoder(&available, |encoder| encoder == VideoEncoder::H264Qsv),
            VideoEncoder::H264Qsv
        );
        assert_eq!(
            select_best_encoder(&available, |encoder| encoder == VideoEncoder::H264Vaapi),
            VideoEncoder::Libx264
        );
    }

    #[test]
    fn opaque_png_frames_drop_alpha_before_rgb_encoding() {
        let rgba = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let (pixels, color_type) = png_pixels(rgba, 2, 1, false).unwrap();

        assert_eq!(pixels, vec![10, 20, 30, 50, 60, 70]);
        assert_eq!(color_type, image::ExtendedColorType::Rgb8);
    }

    #[test]
    fn transparent_png_frames_preserve_rgba() {
        let rgba = vec![10, 20, 30, 40];
        let (pixels, color_type) = png_pixels(rgba.clone(), 1, 1, true).unwrap();

        assert_eq!(pixels, rgba);
        assert_eq!(color_type, image::ExtendedColorType::Rgba8);
    }

    #[test]
    fn png_frames_reject_unexpected_buffer_lengths() {
        let error = png_pixels(vec![0; 3], 1, 1, false).unwrap_err();
        assert!(error.to_string().contains("expected 4 RGBA bytes"));
    }

    #[test]
    fn transparent_export_rejects_formats_without_alpha_contract() {
        assert!(validate_transparency(ExportFormat::Mp4, true).is_err());
        assert!(validate_transparency(ExportFormat::Gif, true).is_err());
        assert!(validate_transparency(ExportFormat::Webm, true).is_ok());
        assert!(validate_transparency(ExportFormat::Webp, true).is_ok());
        assert!(validate_transparency(ExportFormat::PngSequence, true).is_ok());
    }
}
