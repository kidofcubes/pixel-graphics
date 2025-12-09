in vec2 texcoords;
uniform sampler2D fb0tex;
//uniform usampler2D scene;
layout (r32ui) readonly uniform uimage2D computeAccuR;
layout (r32ui) readonly uniform uimage2D computeAccuG;
layout (r32ui) readonly uniform uimage2D computeAccuB;
uniform int frame;


layout (std430) buffer scene
{
// We group the 12 floats into an array of vec4s for clearer alignment.
// NOTE: The struct below must perfectly match the Rust struct's memory layout (48 bytes)
    PixelData scene_pixels[]; // The array of all pixels in your scene
};

PixelData get_pixel(ivec2 pos){
    return scene_pixels[pos.x+(pos.y*RESOLUTIONX)];
}

layout(location = 0) out vec4 pixelColor;


// Simple Reinhard Tone Mapping
// Compresses high light values into the 0-1 range for LDR display
vec3 Reinhard(vec3 color) {
    return color / (color + vec3(1.0));
}


void main() {
    // 1. Calculate pixel position in image space
    ivec2 pixel_coord = ivec2(gl_FragCoord.xy);

    // 2. Read the raw accumulated integer data for the CURRENT FRAME
    // We assume the total number of samples launched in the Compute Shader
    // is RESOLUTIONX * RESOLUTIONY (1 ray per pixel).
    uvec4 r_val = imageLoad(computeAccuR, pixel_coord);
    uvec4 g_val = imageLoad(computeAccuG, pixel_coord);
    uvec4 b_val = imageLoad(computeAccuB, pixel_coord);

    // We only care about the single value in the red channel (x)
    vec3 raw_light_sum = vec3(
    float(r_val.x),
    float(g_val.x),
    float(b_val.x)
    );

    // 3. Normalize the Current Frame's Contribution (New Sample)

    // Convert the summed integer values back to the actual float light value.
    vec3 current_frame_light = raw_light_sum / COMPUTE_MULTIPLIER;

    // Divide by the number of samples that contributed to this pixel.
    // Since this is dynamic, we must use a constant approximation for normalization.
    // Assuming 1 ray per pixel per frame (RESOLUTIONX * RESOLUTIONY samples total)
    float rays_per_pixel = 1.0;
    vec3 normalized_new_sample = current_frame_light / rays_per_pixel;

    // 4. Read the Previous Frame's Accumulated Color (History)
    vec4 history_data = texelFetch(fb0tex, pixel_coord, 0);
    vec3 previous_accumulated_color = history_data.rgb;

    // 5. Exponential Decay / Temporal Accumulation

    // Decay factor (alpha): Determines the weight of the new sample.
    // For a dynamic scene, a high alpha value means a fast update (less ghosting).
    // For a static scene, a low alpha value means smooth convergence.
    // A common way to calculate alpha is based on the reciprocal of the total samples
    // accumulated so far, but since your history is always the previous frame,
    // we use a fixed decay rate (e.g., 5% new sample, 95% history).

    // For dynamic scenes, using a small, fixed alpha is often the best compromise.
    // The history texture already has gamma/tone mapping applied, so we must
    // undo those to mix correctly in linear space.

    // Decay Constant (Alpha): Adjust this value (e.g., 0.1 for 10% new contribution)
    const float ALPHA = 0.1; // 10% new sample, 90% history

    // Undo Gamma Correction from History (assuming sRGB, needed for linear blending)
    vec3 history_linear = pow(previous_accumulated_color, vec3(2.2));

    // Undo Tone Mapping from History (this is tricky, so often skipped or simplified)
    // If you tone map in the post-pass, it's safer to read the linear history.
    // ASSUMPTION: historyTexture stores the final, gamma-corrected color.
    // We will blend the final LDR colors for simplicity and performance,
    // as is common in dynamic game engines.


    // Use the simplified LDR blending (common for dynamic RT)
    vec3 blended_color = mix(previous_accumulated_color, normalized_new_sample, ALPHA);


    // 6. Tone Mapping and Gamma Correction (Applied only to the new blended result)
    // The normalization step (5) now includes the overall accumulation.

    vec3 final_color_linear = blended_color;

    // Reapply Tone Mapping (optional, depends on where you want to tone map)
    final_color_linear = Reinhard(final_color_linear);

    // Gamma Correction (sRGB standard)
    final_color_linear = pow(final_color_linear, vec3(1.0/2.2));

    pixelColor = vec4(final_color_linear, 1.0);
}
