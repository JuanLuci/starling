// Starling Visuals: the only full-screen pass. Pixels at the far plane get the sky and its bodies
// (colortex2: stars, sun and moon over black) in place of the game's own sky; the others the fog
// of their distance, in the sky's colour behind them. Then the film curve.

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

void main() {
    vec3 color = texture2D(colortex0, texcoord).rgb;
    float depth = texture2D(depthtex0, texcoord).r;
    if (depth >= 1.0) {
        // The near plane: the far one is at infinity, where w is 0.
        vec4 view = gbufferProjectionInverse * vec4(texcoord * 2.0 - 1.0, -1.0, 1.0);
        vec3 dir = normalize(mat3(gbufferModelViewInverse) * (view.xyz / view.w));
        color = skyRadiance(dir, sunDir()) + texture2D(colortex2, texcoord).rgb;
    } else {
        vec4 view = gbufferProjectionInverse * vec4(vec3(texcoord, depth) * 2.0 - 1.0, 1.0);
        vec3 rel = mat3(gbufferModelViewInverse) * (view.xyz / view.w);
        color = mix(color, fogColor(rel), fogAmount(rel));
    }
    color = pow(tonemap(color), vec3(1.0 / 2.2));

    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(color, 1.0);
}

#endif
