// Starling Visuals: the lens. Bloom from colortex0's mipmaps (the scene at ever smaller sizes,
// which the game makes before this pass), the sun's rays from where the sky shows on the way to it
// (their alpha, at a small size: soft without many samples), then the film curve.

#include "/lib/common.glsl"

#ifdef VERTEX_SHADER

noperspective out vec2 texcoord;

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    gl_Position = ftransform();
}

#endif

#ifdef FRAGMENT_SHADER

#if defined BLOOM || defined GODRAYS
const bool colortex0MipmapEnabled = true;
#endif

noperspective in vec2 texcoord;

uniform sampler2D colortex0;
uniform mat4 gbufferProjection;
uniform mat4 gbufferProjectionInverse;
uniform float viewWidth;
uniform float viewHeight;

// The mip level that blurs as much as level `level` would on a screen 540 pixels high, so that
// the glow keeps its size whatever the resolution.
float screenLod(float level) {
    return max(level + log2(viewHeight / 540.0), 0.0);
}

// The scene blurred over a few sizes, each read from its mip level four times around the pixel
// (a tent, without the blocks of a single read).
vec3 bloomAt(vec2 uv) {
    vec3 sum = vec3(0.0);
    for (int i = 1; i <= 5; i++) {
        float lod = screenLod(float(i));
        vec2 texel = exp2(lod) / vec2(viewWidth, viewHeight);
        sum += textureLod(colortex0, uv + texel * vec2(0.7, 0.7), lod).rgb
            + textureLod(colortex0, uv + texel * vec2(-0.7, 0.7), lod).rgb
            + textureLod(colortex0, uv + texel * vec2(0.7, -0.7), lod).rgb
            + textureLod(colortex0, uv + texel * vec2(-0.7, -0.7), lod).rgb;
    }
    return sum / 20.0;
}

// The light scattered toward the eye along this pixel's ray by the air between it and the sun or
// moon: how much sky shows on the way to it on screen, in the light's colour, strongest looking
// toward it and when it is low.
vec3 raysAt(vec2 uv) {
    vec4 clip = gbufferProjection * vec4(shadowLightPosition, 1.0);
    if (clip.w <= 0.0) return vec3(0.0);
    vec2 light = clip.xy / clip.w * 0.5 + 0.5;
    vec4 view = gbufferProjectionInverse * vec4(uv * 2.0 - 1.0, -1.0, 1.0);
    float toward = max(dot(normalize(view.xyz), normalize(shadowLightPosition)), 0.0);
    float forward = pow(toward, 8.0);
    if (forward < 0.002) return vec3(0.0);

    float lod = screenLod(2.0);
    vec2 step = (light - uv) / float(GODRAYS_SAMPLES);
    // A different start for neighbouring pixels: the samples' bands become a fine grain.
    float dither = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
    vec2 p = uv + step * dither;
    // Nearer the pixel counts more: a gap's light fades on its way out from it.
    float open = 0.0;
    float weight = 1.0;
    float total = 0.0;
    for (int i = 0; i < GODRAYS_SAMPLES; i++) {
        open += textureLod(colortex0, clamp(p, 0.0, 1.0), lod).a * weight;
        total += weight;
        weight *= 0.88;
        p += step;
    }
    open /= total;

    vec3 s = sunDir();
    float lowSun = 1.0 - smoothstep(0.0, 0.35, abs(s.y));
    float air = 0.25 + 0.75 * lowSun + rainStrength;
    return (sunlight(s) + moonlight(s) * 3.0) * open * forward * air * GODRAYS_STRENGTH * 0.6;
}

void main() {
    vec3 color = texture2D(colortex0, texcoord).rgb;
    #ifdef GODRAYS
        if (isEyeInWater == 0) color += raysAt(texcoord);
    #endif
    #ifdef BLOOM
        color = mix(color, bloomAt(texcoord), BLOOM_STRENGTH);
    #endif
    color = pow(tonemap(color), vec3(1.0 / 2.2));

    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(color, 1.0);
}

#endif
