// Starling Visuals: the world seen from the sun (or moon) into the shadow map, distorted as the
// lookups in lit.glsl undo. Water casts none: the sun goes on lighting what is under it.

#include "/lib/common.glsl"

#ifdef VERTEX_SHADER

out vec2 texcoord;
flat out int material;

attribute vec4 mc_Entity;

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    material = int(mc_Entity.x + 0.5);
    gl_Position = ftransform();
    gl_Position.xy = distortShadow(gl_Position.xy);
}

#endif

#ifdef FRAGMENT_SHADER

in vec2 texcoord;
flat in int material;

uniform sampler2D gtexture;

void main() {
    if (material == 10001 || texture2D(gtexture, texcoord).a < 0.1) discard;

    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(1.0);
}

#endif
