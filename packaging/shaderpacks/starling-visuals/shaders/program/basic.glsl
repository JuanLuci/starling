// Starling Visuals: lines and other untextured shapes (the block outline, leads), unlit.

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

void main() {
    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(toLinear(glcolor.rgb), glcolor.a);
}

#endif
