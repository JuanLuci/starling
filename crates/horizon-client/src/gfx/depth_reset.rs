//! A full-screen draw that writes one depth value and no colour: what a render pass does in
//! place of beginning another with a cleared depth buffer. The first-person hand and the HUD
//! used to take passes of their own after the world's, each loading and storing the whole
//! screen (18 MB each way at 3200x1440; a tiled GPU pays that per pass): drawn in the world's
//! pass after this reset, they cost no load or store at all.

const SHADER: &str = "override DEPTH: f32 = 0.0;
@vertex fn vs_reset(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32(i & 1u) * 4.0 - 1.0, f32(i >> 1u) * 4.0 - 1.0, DEPTH, 1.0);
}
@fragment fn fs_reset() -> @location(0) vec4<f32> { return vec4<f32>(0.0); }";

/// A pipeline that writes `depth` to every pixel of a pass over a `format` target with a
/// Depth32Float attachment: `pass.draw(0..3, 0..1)` (a scissor keeps it to a rectangle).
pub(crate) fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    depth: f32,
    label: &str,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        ..Default::default()
    });
    let constants = [("DEPTH", f64::from(depth))];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_reset"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_reset"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
