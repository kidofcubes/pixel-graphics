
// Accumulation buffers (read-only)
layout(r32ui, binding = 0) uniform highp uimage2D computeAccuR;
layout(r32ui, binding = 1) uniform highp uimage2D computeAccuG;
layout(r32ui, binding = 2) uniform highp uimage2D computeAccuB;

// --- Uniforms from Host ---
uniform int frame;                  // Current frame/sample count (for initial frame check)

// Loopback Buffer Read (Previous Frame's accumulated color)
uniform sampler2D fb0tex;

// --- Scene Data (Layouts match your provided code) ---
layout (std430) buffer scene
{
    PixelData scene_pixels[]; // The array of all pixels in your scene
};

// Assuming PixelData is defined elsewhere and RESOLUTIONX is a constant
PixelData get_pixel(ivec2 pos){
    return scene_pixels[pos.x+(pos.y*RESOLUTIONX)];
}

// Tone mapping functions removed as requested.

// Final output color buffer
layout(location = 0) out vec4 pixelColor;


void main() {
    // 1. Get screen coordinates
    ivec2 pos = ivec2(gl_FragCoord.xy);

    // Safety check: ensure we don't divide by zero
    float samples = max(1.0, float(frame));

    // --- 2. Calculate Illumination for the CURRENT Frame ---

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

    // C. Get the original scene pixel data and add emission
    PixelData scenePixel = get_pixel(pos);
    vec3 emission = scenePixel.color.rgb * scenePixel.emissiveness * 10.0;

    // D. Calculate the final color for *this frame only*
    vec3 currentFrameColor = scenePixel.color.rgb * (currentFrameIllumination + emission);


    // --- 3. Accumulate with Previous Frame (Exponential Moving Average) ---

    if (frame <= 1) {
        // First frame: Initialize the buffer with the first sample
        // We only apply Gamma Correction, as Tone Mapping is removed.
        vec3 finalColor = pow(currentFrameColor, vec3(1.0/2.2));
        pixelColor = vec4(finalColor, scenePixel.color.a);

    } else {
        // Subsequent frames: Use Exponential Moving Average (EMA)

        // Read the accumulated color (which is LDR/Gamma) from the previous frame's FBO texture
        vec4 previousFrameData = texelFetch(fb0tex, pos, 0);
        vec3 previousFrameLDR = previousFrameData.rgb;

        // --- CONVERT PREVIOUS COLOR BACK TO HDR/LINEAR SPACE ---

        // 1. Reverse Gamma Correction (Gamma -> Linear)
        // This takes the LDR color back into the linear/HDR space for correct mixing.
        vec3 previousFrameHDR = pow(previousFrameLDR, vec3(2.2));

        // 2. Accumulation (Blending in Linear/HDR Space)

        // Fixed EMA Blending Factor (Alpha):
        float alpha = 0.05;

        // Blend: new = (1 - alpha) * old_HDR + alpha * new_HDR
        vec3 blendedHDRColor = mix(previousFrameHDR, currentFrameColor, alpha);

        // --- Final Output Processing (Gamma Correction Only) ---

        // 3. Apply Gamma Correction (standard for display)
        vec3 finalColor = pow(blendedHDRColor, vec3(1.0/2.2));

        // 4. Output the Final Color
        pixelColor = vec4(finalColor, scenePixel.color.a);
    }
}