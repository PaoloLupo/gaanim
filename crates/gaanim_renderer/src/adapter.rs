//! Adapter selection for the GPU contexts that render without a window:
//! exports, snapshot captures and shader backgrounds.

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
