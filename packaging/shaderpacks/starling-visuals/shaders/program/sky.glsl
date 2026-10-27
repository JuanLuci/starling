// Starling Visuals: the vanilla sky's geometry. The sky's colour itself is drawn by `final` (one
// evaluation per sky pixel); here only the stars stay, in colortex2 with the sun and moon.

#include "/lib/common.glsl"

#ifdef VERTEX_SHADER

out vec4 glcolor;

void main() {
    glcolor = gl_Color;
    gl_Position = ftransform();
}

#endif

#ifdef FRAGMENT_SHADER

in vec4 glcolor;

uniform int renderStage;

void main() {
    if (renderStage != MC_RENDER_STAGE_STARS) discard;
    float night = 1.0 - daylight(sunDir());

    /* DRAWBUFFERS:2 */
    gl_FragData[0] = vec4(toLinear(glcolor.rgb) * 2.0 * night * (1.0 - rainStrength), glcolor.a);
}

#endif
