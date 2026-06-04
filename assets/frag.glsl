// varying vec3 v_color;
in vec2 texcoords;
layout(location = 0) out vec4 pixelColor;
//uniform usampler2D scene;
//uniform usampler2D computeAccu; // integer hit count
uniform sampler2D fb0tex;

void main() {
    // gl_FragColor = vec4(v_color, 1.0);
//    pixelColor = vec4(texcoords.xy, 0.0, 1.0);
//    pixelColor = vec4(texelFetch(scene, pos, 0));
//    pixelColor = vec4(texture(scene, texcoords, 0));
//    pixelColor = texture(scene, texcoords, 0);
//    uvec4 pixel_data = texture(scene, texcoords, 0);
//    uint color_data_bits = floatBitsToUint(pixel_data.x);
//    uint color_data_bits = pixel_data.x;
//    uint color_data_bits = 0;
//    vec4 color_data = vec4(
//        float((color_data_bits >> 24) | uint(255))/255.0,
//        float((color_data_bits >> 16) | uint(255))/255.0,
//        float((color_data_bits >>  8) | uint(255))/255.0,
//        float((color_data_bits >>  0) | uint(255))/255.0
//    );
//    vec4 color_data = unpackUnorm4x8(color_data_bits);
//    vec4 color_data = vec4(1.0);
//    vec4 color_data = unpackUnorm4x8(color_data_bits);

    vec4 color_data = vec4(0.0,0.0,0.0,0.7);
    ivec2 pos = ivec2(texcoords.x*RESOLUTIONX,texcoords.y*RESOLUTIONY);
//    uvec4 value = texelFetch(computeAccu, pos, 0);
//    uvec4 value = texture(computeAccu, texcoords.xy, 0);
//    vec4 value = texture(computeAccu, texcoords.xy, 0);
//    uvec4 value = texture(computeAccu, texcoords.xy, 0);
//    uvec4 light_value = texelFetch(computeAccu, pos, 0);
//    vec4 value = texture(fb0tex, texcoords.xy, 0);
    vec4 value = texelFetch(fb0tex, pos, 0);
    color_data = value;
//    color_data = pow(value,vec4(1.0 / 2.2));
//    color_data.rgb = vec3(float(value.x)/100.0);
//    color_data.r = 0.1;
//    color_data.g = 0.1;
//    color_data.b = 0.1;
//    color_data.w = 0.5;
//    pixel_data.w = 255*255*255*255;
//    pixelColor = color_data;
//    pixelColor = vec4(color_data) / 255.0;
    pixelColor = vec4(color_data);
//    pixelColor = vec4(0.5);
    // gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);
}
