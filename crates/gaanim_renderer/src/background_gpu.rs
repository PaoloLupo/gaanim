//! Draws shader backgrounds on the render device that Vello renders with.

use std::sync::{Arc, Mutex, Weak};

use bevy::prelude::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy_vello::render::VelloRenderer;

use crate::background::{GpuShaderBackgrounds, ShaderBackgroundRequest};

/// Shader background drawn by the latest composed scene.
///
/// Present only when a render world draws it on the GPU; otherwise the scene
/// falls back to a CPU copy of the shader output.
#[derive(Resource, Default)]
pub struct ShaderBackgroundFrame(pub(crate) Option<ShaderBackgroundRequest>);

#[derive(Resource, Default)]
struct ExtractedShaderBackground(Option<ShaderBackgroundRequest>);

#[derive(Resource, Default)]
struct RenderShaderBackgrounds {
    backgrounds: GpuShaderBackgrounds,
    /// Renderer holding the texture overrides. The weak reference keeps its
    /// address from being reused by a replacement renderer.
    renderer: Weak<Mutex<vello::Renderer>>,
}

pub(crate) fn build(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExtractedShaderBackground>()
        .init_resource::<RenderShaderBackgrounds>()
        .add_systems(ExtractSchedule, extract_shader_background)
        .add_systems(
            Render,
            prepare_shader_background
                .in_set(RenderSystems::PrepareResources)
                .run_if(resource_exists::<RenderDevice>)
                .run_if(resource_exists::<VelloRenderer>),
        );
    app.init_resource::<ShaderBackgroundFrame>();
}

fn extract_shader_background(
    frame: Extract<Option<Res<ShaderBackgroundFrame>>>,
    mut extracted: ResMut<ExtractedShaderBackground>,
) {
    extracted.0 = frame.as_ref().and_then(|frame| frame.0.clone());
}

fn prepare_shader_background(
    extracted: Res<ExtractedShaderBackground>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    vello_renderer: Res<VelloRenderer>,
    mut state: ResMut<RenderShaderBackgrounds>,
) {
    let shared: &Arc<Mutex<vello::Renderer>> = &vello_renderer;
    let Ok(mut renderer) = shared.lock() else {
        return;
    };
    let state = &mut *state;
    if !Weak::ptr_eq(&state.renderer, &Arc::downgrade(shared)) {
        state
            .backgrounds
            .renderer_replaced(device.wgpu_device(), &mut renderer);
        state.renderer = Arc::downgrade(shared);
    }
    state.backgrounds.prepare(
        device.wgpu_device(),
        &queue,
        &mut renderer,
        extracted.0.as_ref(),
    );
}
