in vec2 texcoords;
uniform sampler2D fb0tex;
uniform usampler2D scene;
uniform usampler2D computeAccu;
uniform int frame;
layout(location = 0) out vec4 pixelColor;

void main() {
    ivec2 pos = ivec2(gl_FragCoord);
    vec4 prev = texelFetch(fb0tex, pos, 0);
//    ivec2 pos = ivec2(texcoords.x*RESOLUTIONX,texcoords.y*RESOLUTIONY);

    float blend = 1.0 / float(KEEP_FRAMES);


    uvec4 scene_data = texelFetch(scene, pos, 0);

    vec4 unpackedColors = unpackUnorm4x8(scene_data.x);

    unpackedColors *= float(texelFetch(computeAccu, pos, 0).x)/float(COMPUTE_MULTIPLIER);


    vec4 accum = mix(prev, unpackedColors, blend);
    accum.w = 1.0;
    pixelColor = accum;
}
