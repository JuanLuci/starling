// Starling Visuals: the sun and moon textures, in colortex2 (the sky's bodies, black elsewhere),
// bright enough to stand out of the sky `final` puts behind them.

#include "/lib/common.glsl"

#ifdef VERTEX_SHADER

out vec2 texcoord;
out vec4 glcolor;

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    glcolor = gl_Color;
    gl_Position = ftransform();
}

#endif

#ifdef FRAGMENT_SHADER

in vec2 texcoord;
in vec4 glcolor;

uniform sampler2D gtexture;
uniform int renderStage;

void main() {
    vec4 body = texture2D(gtexture, texcoord) * glcolor;
    float strength = renderStage == MC_RENDER_STAGE_SUN ? 6.0 : 1.2;

    /* DRAWBUFFERS:2 */
    gl_FragData[0] = vec4(toLinear(body.rgb) * strength * (1.0 - rainStrength), body.a);
}

#endif
