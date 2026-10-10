//! Adapter selection for the GPU contexts that render without a window
//! (exports, snapshot captures and shader backgrounds), and the backends the
//! preview window starts.

use vello::wgpu;

/// Backends searched first when `WGPU_BACKEND` is unset: each platform's
/// native API. An instance of every backend also starts DX12 and GL, which
/// costs about half a second on Windows before the first frame.
const NATIVE_BACKENDS: wgpu::Backends = wgpu::Backends::VULKAN.union(wgpu::Backends::METAL);

/// A high-performance adapter for headless rendering.
///
/// `WGPU_BACKEND` (vulkan, dx12, metal, gl) narrows the search as it does
/// for the preview window; CI uses it to skip broken adapters. Otherwise the
/// native backends are searched first, and every backend only when they
/// offer no hardware adapter.
pub fn request_headless_adapter() -> Result<wgpu::Adapter, wgpu::RequestAdapterError> {
    if let Some(backends) = wgpu::Backends::from_env() {
        return request_adapter(backends);
    }
    if let Ok(adapter) = request_adapter(NATIVE_BACKENDS)
        && adapter.get_info().device_type != wgpu::DeviceType::Cpu
    {
        return Ok(adapter);
    }
    request_adapter(wgpu::Backends::all())
}

/// The probe [`start_window_backends_probe`] started, if it has not been
/// read yet.
#[cfg(not(target_arch = "wasm32"))]
static WINDOW_BACKENDS_PROBE: std::sync::Mutex<
    Option<std::thread::JoinHandle<Option<wgpu::Backends>>>,
> = std::sync::Mutex::new(None);

/// Start looking for the preview window's backends on another thread, so the
/// search overlaps the rest of startup; [`window_backends`] then waits for it.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_window_backends_probe() {
    let Ok(mut probe) = WINDOW_BACKENDS_PROBE.lock() else {
        return;
    };
    if probe.is_none() {
        *probe = std::thread::Builder::new()
            .name("gaanim-gpu-probe".into())
            .spawn(probe_window_backends)
            .ok();
    }
}

/// The backends the preview window starts when `WGPU_BACKEND` is unset: each
/// platform's native API when it offers a hardware adapter, or `None` for
/// every backend, as before. Starting DX12 next to Vulkan costs ~0.3 s on
/// Windows, and at times seconds, though Vulkan is the backend picked.
#[cfg(not(target_arch = "wasm32"))]
pub fn window_backends() -> Option<wgpu::Backends> {
    let started = WINDOW_BACKENDS_PROBE
        .lock()
        .ok()
        .and_then(|mut probe| probe.take());
    match started {
        Some(probe) => probe.join().ok().flatten(),
        None => probe_window_backends(),
    }
}

/// See [`window_backends`]. Requesting the native adapter also loads its
/// driver, which the window's own instance then starts faster.
#[cfg(not(target_arch = "wasm32"))]
fn probe_window_backends() -> Option<wgpu::Backends> {
    if wgpu::Backends::from_env().is_some() {
        return None;
    }
    let adapter = request_adapter(NATIVE_BACKENDS).ok()?;
    (adapter.get_info().device_type != wgpu::DeviceType::Cpu).then_some(NATIVE_BACKENDS)
}

fn request_adapter(backends: wgpu::Backends) -> Result<wgpu::Adapter, wgpu::RequestAdapterError> {
    let mut flags = wgpu::InstanceFlags::default();
    // Vello dispatches indirectly every frame, and wgpu validates those
    // arguments with extra passes that only DX12 needs; Bevy drops the flag
    // for the preview window on the same condition.
    if !backends.contains(wgpu::Backends::DX12) {
        flags.remove(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL);
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: flags.with_env(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
}
