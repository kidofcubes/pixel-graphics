#version 460
//precision mediump float;

// varying vec3 v_color;
in vec2 texcoords;
layout(location = 0) out vec4 pixelColor;

void main() {
    // gl_FragColor = vec4(v_color, 1.0);
    pixelColor = vec4(texcoords.xy, 0.0, 1.0);
    // gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);
}
