// Starling Visuals: everything drawn with the world's light, in one forward pass. The light is
// worked out per vertex (faces are flat, the lightmap is per vertex anyway); the fragment stage
// samples, looks the sun up in the shadow map and blends, and water adds its reflection.
// `composite` adds the haze. The stage file defines VERTEX_SHADER or FRAGMENT_SHADER, and what it
// draws: TERRAIN, WATER, ENTITIES, CLOUDS, PARTICLES, WEATHER or nothing (other textured things).

#include "/lib/common.glsl"

#if defined SHADOWS && !defined CLOUDS
    #define SHADOWED
#endif

#ifdef VERTEX_SHADER

out vec2 texcoord;
out vec4 glcolor;
out vec3 fill;
out vec3 key;
out float open;
flat out int material;
#ifdef SHADOWED
out vec3 shadowClip;
#endif
#ifdef WATER
out vec3 rel;
out vec3 normal;
out float skyLevel;
#endif

attribute vec4 mc_Entity;

uniform mat4 shadowModelView;
uniform mat4 shadowProjection;

// The face shade the game bakes into terrain colours (1 up, 0.5 down, 0.8 north and south,
// 0.6 east and west), which this pack's own sun replaces.
float bakedShade(vec3 n) {
    vec3 a = abs(n);
    return a.x * 0.6 + a.z * 0.8 + (n.y > 0.0 ? a.y : a.y * 0.5);
}

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    glcolor = gl_Color;
    material = int(mc_Entity.x + 0.5);
    vec4 position = gl_ModelViewMatrix * gl_Vertex;
    gl_Position = gl_ProjectionMatrix * position;

    vec3 n = normalize(mat3(gbufferModelViewInverse) * (gl_NormalMatrix * gl_Normal));
    // The whole matrix: the view bob is in it, translation and all.
    vec3 at = (gbufferModelViewInverse * position).xyz;
    vec2 levels = lightLevels((gl_TextureMatrix[1] * gl_MultiTexCoord1).xy);
    float wrap = 0.0;
    float unshade = 1.0;
    #if defined TERRAIN || defined WATER
        if (material == 10002 || material == 10003) {
            wrap = 1.0;
        } else {
            unshade = 1.0 / max(bakedShade(n), 0.5);
        }
    #endif
    #ifdef CLOUDS
        levels = vec2(0.0, 1.0);
        wrap = 0.5;
    #endif
    #if defined PARTICLES || defined WEATHER
        wrap = 1.0;
    #endif

    fill = fillLightAt(n, levels) * unshade;
    key = keyLightAt(n, wrap) * unshade;
    #ifdef CLOUDS
        // Their texture is white; the cloud itself sends back about half the light.
        fill *= 0.55;
        key *= 0.55;
    #endif
    open = openSky(levels);
    #ifdef SHADOWED
        // Off the surface a little, along its normal (toward the light for plants, whose normals
        // point anywhere), so that it doesn't shadow itself; further with distance, where the
        // map's texels are bigger.
        vec3 off = mix(n, keyLightDir(), wrap) * (0.03 + length(at) * 0.004);
        shadowClip = (shadowProjection * (shadowModelView * vec4(at + off, 1.0))).xyz;
    #endif
    #ifdef WATER
        rel = at;
        normal = n;
        skyLevel = levels.y;
    #endif
}

#endif

#ifdef FRAGMENT_SHADER

in vec2 texcoord;
in vec4 glcolor;
in vec3 fill;
in vec3 key;
in float open;
flat in int material;
#ifdef SHADOWED
in vec3 shadowClip;
#endif
#ifdef WATER
in vec3 rel;
in vec3 normal;
in float skyLevel;
#endif

uniform sampler2D gtexture;
uniform vec4 entityColor;
#ifdef SHADOWED
uniform sampler2DShadow shadowtex0;
#endif
#ifdef WATER
uniform sampler2D depthtex1;
uniform mat4 gbufferProjectionInverse;
uniform vec3 cameraPosition;
uniform float frameTimeCounter;
uniform float viewWidth;
uniform float viewHeight;

// The open water's normal at world point `p` (x, z) and time `t`: three travelling waves, each
// leaning the surface along its direction by its slope.
vec3 waveNormal(vec2 p, float t) {
    vec2 a = vec2(0.80, 0.60);
    vec2 b = vec2(-0.50, 0.87);
    vec2 c = vec2(0.20, -0.98);
    vec2 slope = a * cos(dot(a, p) * 1.3 + t * 1.1) * 0.026
        + b * cos(dot(b, p) * 2.1 + t * 1.7) * 0.025
        + c * cos(dot(c, p) * 3.7 + t * 2.3) * 0.026;
    return normalize(vec3(-slope.x, 1.0, -slope.y));
}
#endif

// How much of the sun reaches this point: the shadow map within its reach, the open sky past it.
// Each lookup is filtered by the hardware over four texels; SHADOW_FILTER 4 takes four of them
// on a rotated grid, which softens the edges and their crawl as the sun moves.
float sunReaches() {
    #ifdef SHADOWED
        float reach = max(abs(shadowClip.x), abs(shadowClip.y));
        if (reach < 1.0) {
            vec3 at = vec3(distortShadow(shadowClip.xy), shadowClip.z) * 0.5 + 0.5;
            at.z -= 0.0002;
            #if SHADOW_FILTER == 4
                // SHADOW_SOFTNESS texels out, on a grid turned by about 18 degrees.
                float radius = SHADOW_SOFTNESS / float(shadowMapResolution);
                float lit = 0.25 * (
                    shadow2D(shadowtex0, at + vec3(vec2(0.95, 0.32) * radius, 0.0)).r
                    + shadow2D(shadowtex0, at + vec3(vec2(-0.32, 0.95) * radius, 0.0)).r
                    + shadow2D(shadowtex0, at + vec3(vec2(-0.95, -0.32) * radius, 0.0)).r
                    + shadow2D(shadowtex0, at + vec3(vec2(0.32, -0.95) * radius, 0.0)).r);
            #else
                float lit = shadow2D(shadowtex0, at).r;
            #endif
            return mix(lit, open, smoothstep(0.85, 1.0, reach));
        }
    #endif
    return open;
}

void main() {
    vec4 albedo = texture2D(gtexture, texcoord) * glcolor;
    if (albedo.a < 0.1) discard;
    #ifdef ENTITIES
        albedo.rgb = mix(albedo.rgb, entityColor.rgb, entityColor.a);
    #endif

    float sun = dot(key, vec3(1.0)) > 0.0 ? sunReaches() : 0.0;
    vec3 color = toLinear(albedo.rgb) * (fill + key * sun);
    float alpha = albedo.a;

    #ifdef WATER
        if (material == 10001) {
            vec3 s = sunDir();
            vec3 n = normalize(normal);
            vec3 view = normalize(rel);
            // Its open surface moves with waves; the sides of falling water stay flat.
            if (n.y > 0.9) n = waveNormal(rel.xz + cameraPosition.xz, frameTimeCounter);
            vec3 mirrored = reflect(view, n);
            mirrored.y = abs(mirrored.y);
            float fresnel = 0.02 + 0.98 * pow(1.0 - clamp(dot(-view, n), 0.0, 1.0), 5.0);
            vec3 reflected = skyRadiance(mirrored, s) * skyLevel;
            float glint = pow(clamp(dot(mirrored, keyLightDir()), 0.0, 1.0), 600.0);
            // How much water the eye looks through to the bottom (the depth without water): clear
            // where shallow, its own colour where deep.
            float bottom = texture2D(depthtex1, gl_FragCoord.xy / vec2(viewWidth, viewHeight)).r;
            vec4 under = gbufferProjectionInverse * vec4(vec3(gl_FragCoord.xy / vec2(viewWidth, viewHeight), bottom) * 2.0 - 1.0, 1.0);
            float through = bottom >= 1.0 ? 64.0 : max(length(under.xyz / under.w) - length(rel), 0.0);
            float body = clamp(1.0 - exp(-through * 0.35), 0.12, 0.92);
            color = mix(color * 0.45, reflected, fresnel) + sunlight(s) * glint * 6.0 * sunReaches();
            alpha = mix(body, 1.0, fresnel);
        }
    #endif
    #ifdef WEATHER
        alpha *= 0.6;
    #endif

    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(color, alpha);
}

#endif
