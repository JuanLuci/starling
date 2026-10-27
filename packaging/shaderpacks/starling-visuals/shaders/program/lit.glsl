// Starling Visuals: everything drawn with the world's light, in one forward pass. The light is
// worked out per vertex (faces are flat, the lightmap is per vertex anyway), so the fragment stage
// only samples, multiplies and blends; water adds its reflection per fragment. `final` adds fog.
// The stage file defines VERTEX_SHADER or FRAGMENT_SHADER, and what it draws: TERRAIN, WATER,
// ENTITIES, CLOUDS, PARTICLES, WEATHER or nothing (other textured things).

#include "/lib/common.glsl"

#ifdef VERTEX_SHADER

out vec2 texcoord;
out vec4 glcolor;
out vec3 light;
flat out int material;
#ifdef WATER
out vec3 rel;
out vec3 normal;
out float skyLevel;
#endif

attribute vec4 mc_Entity;

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
    vec3 at = mat3(gbufferModelViewInverse) * position.xyz;
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

    light = lightAt(n, levels, wrap) * unshade;
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
in vec3 light;
flat in int material;
#ifdef WATER
in vec3 rel;
in vec3 normal;
in float skyLevel;
#endif

uniform sampler2D gtexture;
uniform vec4 entityColor;

void main() {
    vec4 albedo = texture2D(gtexture, texcoord) * glcolor;
    if (albedo.a < 0.1) discard;
    #ifdef ENTITIES
        albedo.rgb = mix(albedo.rgb, entityColor.rgb, entityColor.a);
    #endif

    vec3 color = toLinear(albedo.rgb) * light;
    float alpha = albedo.a;

    #ifdef WATER
        if (material == 10001) {
            vec3 s = sunDir();
            vec3 n = normalize(normal);
            vec3 view = normalize(rel);
            vec3 mirrored = reflect(view, n);
            float fresnel = 0.02 + 0.98 * pow(1.0 - clamp(dot(-view, n), 0.0, 1.0), 5.0);
            vec3 reflected = skyRadiance(mirrored, s) * skyLevel;
            float glint = pow(clamp(dot(mirrored, keyLightDir()), 0.0, 1.0), 300.0);
            color = mix(color * 0.6, reflected, fresnel) + sunlight(s) * glint * 4.0 * skyLevel;
            alpha = mix(0.55, 1.0, fresnel);
        }
    #endif
    #ifdef WEATHER
        alpha *= 0.6;
    #endif

    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(color, alpha);
}

#endif
