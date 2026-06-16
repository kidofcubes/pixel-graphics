
// Accumulation buffers (read-only)
layout(r32ui, binding = COMPUTEACCUR) uniform highp uimage2D computeAccuR;
layout(r32ui, binding = COMPUTEACCUG) uniform highp uimage2D computeAccuG;
layout(r32ui, binding = COMPUTEACCUB) uniform highp uimage2D computeAccuB;

// --- Uniforms from Host ---
layout(location = FRAME)        uniform int frame;

// Loopback Buffer Read (Previous Frame's accumulated color)
layout(binding = FB0TEX) uniform sampler2D fb0tex;

// --- Scene Data (Layouts match your provided code) ---
layout (std430, binding = SCENE) buffer scene
{
    PixelData scene_pixels[]; // The array of all pixels in your scene
};

// Assuming PixelData is defined elsewhere and RESOLUTIONX is a constant
PixelData get_pixel(ivec2 pos){
    return scene_pixels[pos.x+(pos.y*RESOLUTIONX)];
}

// Final output color buffer
layout(location = 0) out vec4 pixelColor;


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
    // 1. Get screen coordinates
    ivec2 pos = ivec2(gl_FragCoord.xy);

    // A. Read raw light counts from the current frame's path tracing
    uint r_count = imageLoad(computeAccuR, pos).r;
    uint g_count = imageLoad(computeAccuG, pos).r;
    uint b_count = imageLoad(computeAccuB, pos).r;

    // B. Calculate the *current frame's* raw illumination contribution (unblended)
    // We only scale by COMPUTE_MULTIPLIER. Result is in HDR/Linear space.
    vec3 currentFrameIllumination = vec3(
    float(r_count),
    float(g_count),
    float(b_count)
    ) / float(COMPUTE_MULTIPLIER);
//    currentFrameIllumination = vec3(
//        clamp(currentFrameIllumination.r, 0.0, 1.0),
//        clamp(currentFrameIllumination.g, 0.0, 1.0),
//        clamp(currentFrameIllumination.b, 0.0, 1.0)
//    );

    // C. Get the original scene pixel data and add emission
    PixelData scenePixel = get_pixel(pos);

    // D. Calculate the final color for *this frame only*
//    vec3 currentFrameColor = scenePixel.color.rgb * (currentFrameIllumination + emission);
    vec3 currentFrameColor = scenePixel.color.rgb * (currentFrameIllumination);
//    currentFrameColor = currentFrameIllumination;
    //currentFrameColor = (scenePixel.color.rgb*0.5*scenePixel.color.a)+currentFrameIllumination;
//    currentFrameColor = (scenePixel.color.rgb*0.5)+currentFrameIllumination;
    currentFrameColor = currentFrameIllumination;


    // --- 3. Accumulate with Previous Frame (Exponential Moving Average) ---

    // Subsequent frames: Use Exponential Moving Average (EMA)

    // Read the accumulated color (which is LDR/Gamma) from the previous frame's FBO texture
    vec4 previousFrameData = texelFetch(fb0tex, pos, 0);
    vec3 previousFrameLDR = previousFrameData.rgb;
//    float alpha = 0.001;
    float alpha = 0.01;
//    float alpha = 1.0;
//    float alpha = 0.1;
    pixelColor = vec4(mix(previousFrameLDR,currentFrameColor,alpha), 1.0);

}