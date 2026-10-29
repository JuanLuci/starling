// RenderPipelines.STARS: positions only, coloured by the star brightness.
struct Stars {
    // Projection * view * the sky's rotation (SkyRenderer.renderSunMoonAndStars).
    clip_from_sky: mat4x4<f32>,
    // starBrightness in every channel (RenderSystem.setShaderColor).
    color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> stars: Stars;

// On the far plane (clip z 0, reverse-Z): drawn after the opaque terrain with a GreaterEqual
// depth test, the stars show where no terrain is, as they did drawn before it.
@vertex fn vs_stars(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    let clip = stars.clip_from_sky * vec4(position, 1.0);
    return vec4(clip.xy, 0.0, clip.w);
}

@fragment fn fs_stars() -> @location(0) vec4<f32> {
    return stars.color;
}
