// varying vec3 v_color;
in vec2 texcoords;
layout(location = 0) out vec4 pixelColor;
//uniform usampler2D scene;
//uniform usampler2D computeAccu; // integer hit count
layout(binding = FB0TEX) uniform sampler2D fb0tex;

layout(std430, binding = SCENE) readonly restrict buffer scene {
    PixelData scene_pixels[];
};


const float horizon_y = 111.0;
const float sky_height = 360.0;

vec3 background_sky_color(vec2 pixel_pos, vec2 sun_dir, vec3 sun_color, float time_of_day) {
    float height_t = (pixel_pos.y - horizon_y) / sky_height; // 0 at horizon, 1 at top

    // Zenith vs horizon gradient (tune these colors per time-of-day)
    vec3 zenith_color  = vec3(0.25, 0.55, 0.95);
    vec3 horizon_color = vec3(0.75, 0.85, 0.95);
    vec3 base = mix(horizon_color, zenith_color, pow(height_t, 0.5));

    // Glow toward the sun (fake Mie-like halo)
    vec2 sun_pos = vec2(640.0, 360.0)*0.5;
    vec2 to_sun = normalize(sun_pos - pixel_pos);
    float sun_dot = dot(to_sun, -sun_dir); // how aligned this pixel is with sun direction
//    float sun_dot = 1.0; // how aligned this pixel is with sun direction
    float glow_strength = 1.0;
    vec3 sun_glow = sun_color * pow(max(sun_dot, 0.0), 8.0) * glow_strength;

    return base;
}


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
    if(vec2(pos).y >= horizon_y){
        PixelData pixelData = scene_pixels[pos.x + (pos.y * RESOLUTIONX)];
        if(pixelData.density == 0){
//            pixelColor = vec4(background_sky_color(vec2(pos), normalize(vec2(-0.5, -0.5)), vec3(1.0, 0.9, 0.6), 0.0), 1.0);
        }
    }
//    pixelColor = vec4(0.5);
    // gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);
}
