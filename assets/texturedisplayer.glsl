in vec2 texcoords;
layout(location = 0) out vec4 pixelColor;
uniform usampler2D scene;
uniform sampler2D fb0tex;

void main() {
    uvec4 scene_data = texture(scene, texcoords, 0);
    vec4 unpackedColors = unpackUnorm4x8(scene_data.x);
    pixelColor = texture(fb0tex, texcoords, 0);
//    pixelColor = vec4(texcoords.x, texcoords.y, 0.0, 1.0);
//    pixelColor = unpackedColors;
}
