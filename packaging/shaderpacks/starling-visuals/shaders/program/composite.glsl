// Starling Visuals: the sky and the air. Pixels at the far plane get the sky, the sun and the other
// bodies (colortex2: stars and moon over black) in place of the game's own sky; the others the haze
// they are seen through, thicker low down, and past most of the render distance the sky itself.
// The alpha is how much of the sky shows there, which `final` follows to the sun for its rays.

#include "/lib/common.glsl"

/*
const int colortex0Format = RGBA16F;
const int colortex2Format = RGBA16F;
const bool colortex2Clear = true;
*/

#ifdef VERTEX_SHADER

noperspective out vec2 texcoord;

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    gl_Position = ftransform();
}

#endif

#ifdef FRAGMENT_SHADER

noperspective in vec2 texcoord;

uniform sampler2D colortex0;
uniform sampler2D colortex2;
uniform sampler2D depthtex0;
uniform mat4 gbufferProjectionInverse;
uniform vec3 cameraPosition;
uniform ivec2 eyeBrightnessSmooth;

// The haze is FOG_DENSITY thick at sea level and thins by e every FOG_HEIGHT blocks above it.
const float FOG_FLOOR = 63.0;
const float FOG_HEIGHT = 40.0;

// How much haze a ray `rel` long, from the eye at height `y0`, crosses: thicker at dawn and dusk
// and in rain, and no more than twice sea level's deep underground.
float hazeDepth(vec3 rel, float y0, vec3 s) {
    float lowSun = 1.0 - smoothstep(0.0, 0.35, abs(s.y));
    float density = FOG_DENSITY * 0.003 * (1.0 + 0.5 * lowSun + 4.0 * rainStrength);
    float e0 = exp(-(y0 - FOG_FLOOR) / FOG_HEIGHT);
    float k = -rel.y / FOG_HEIGHT;
    // The mean of exp(-(y - FOG_FLOOR) / FOG_HEIGHT) along the ray.
    float mean = abs(k) > 1e-3 ? e0 * (exp(k) - 1.0) / k : e0;
    return density * min(mean, 2.0) * length(rel);
}

// The light the haze along `d` sends to the eye: the sky's near the horizon (looking down at a
// valley, the sunlit air is as bright as the horizon), dark where the eye has no sky above.
vec3 hazeLight(vec3 d, vec3 s) {
    vec3 level = normalize(vec3(d.x, max(d.y, 0.0) + 0.001, d.z));
    float eyeSky = float(eyeBrightnessSmooth.y) / 240.0;
    return skyRadiance(level, s) * mix(0.05, 1.0, eyeSky * eyeSky);
}

void main() {
    vec3 color = texture2D(colortex0, texcoord).rgb;
    float depth = texture2D(depthtex0, texcoord).r;
    vec3 s = sunDir();
    float sky = 1.0;
    if (depth >= 1.0) {
        // The near plane: the far one is at infinity, where w is 0.
        vec4 view = gbufferProjectionInverse * vec4(texcoord * 2.0 - 1.0, -1.0, 1.0);
        vec3 dir = normalize(mat3(gbufferModelViewInverse) * (view.xyz / view.w));
        // A round sun, soft at its rim (the bloom makes its glow), and the moon and stars.
        float disk = smoothstep(0.99895, 0.99935, dot(dir, s)) * (1.0 - rainStrength);
        color = skyRadiance(dir, s) + sunlight(s) * disk * 8.0 + texture2D(colortex2, texcoord).rgb;
        if (isEyeInWater == 1) color = vec3(0.02, 0.11, 0.15) * (0.15 + 0.85 * daylight(s));
    } else {
        vec4 view = gbufferProjectionInverse * vec4(vec3(texcoord, depth) * 2.0 - 1.0, 1.0);
        vec3 rel = (gbufferModelViewInverse * vec4(view.xyz / view.w, 1.0)).xyz;
        float dist = length(rel);
        vec3 dir = rel / max(dist, 1e-4);
        if (isEyeInWater == 1) {
            vec3 water = vec3(0.02, 0.11, 0.15) * (0.15 + 0.85 * daylight(s));
            color = mix(color, water, 1.0 - exp(-dist * 0.07));
        } else if (isEyeInWater == 2) {
            color = mix(color, vec3(0.9, 0.25, 0.02), 1.0 - exp(-dist * 0.8));
        } else if (isEyeInWater == 3) {
            color = mix(color, vec3(0.55, 0.62, 0.70), 1.0 - exp(-dist * 0.6));
        } else {
            float haze = 1.0 - exp(-hazeDepth(rel, cameraPosition.y, s));
            color = mix(color, hazeLight(dir, s), haze);
            // The world's edge fades into the sky behind it.
            float edge = smoothstep(far * 0.7, far, length(rel.xz));
            color = mix(color, skyRadiance(dir, s), edge);
            sky = edge * edge;
        }
    }

    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(color, isEyeInWater == 0 ? sky : 0.0);
}

#endif
