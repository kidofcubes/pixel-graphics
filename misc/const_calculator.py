import numpy as np

# 1. Define the rectangle corners (adjust these coordinates as needed)
# Example: A 2x4 rectangle centered at the origin
width = 640.0
height = 360.0
num_angles = 64  # Change this to however many steps you need
corners = np.array([
    [0, 0],
    [width, 0],
    [width, height],
    [0, height]
])

# 2. Generate uniformly placed angles from 0.5*pi to 1.5*pi
orig_angles = np.linspace(1.0 * np.pi, 2.0 * np.pi, num_angles)
# angles = np.linspace(0.5 * np.pi, 1.5 * np.pi, num_angles)
angles = orig_angles-(np.pi/2.0)

lengths = []
mins_maxes = []  # To store the vec2 pairs
previous_length = 0.0

# 3. Compute values
for angle in angles:
    unit_vector = np.array([np.cos(angle), np.sin(angle)])
    projected_corners = np.dot(corners, unit_vector)

    # Get min and max projection values
    min_val = np.min(projected_corners)
    max_val = np.max(projected_corners)
    mins_maxes.append([min_val, max_val])

    # Accumulate length
    span = max_val - min_val
    current_length = span + previous_length
    lengths.append(current_length)
    previous_length = current_length

# 4. Format and print GLSL Arrays
print(f"// GLSL Ready Arrays (Size: {num_angles})")
print("====================================")


# Print float array
glsl_float_str = f"const float SKY_DIRECTIONS[{num_angles}] = float[](\n"
glsl_float_str += ",\n".join([f"    {l:.10f}" for l in orig_angles])
glsl_float_str += "\n);"
print(glsl_float_str)

print("\n")

# Print vec2 array
glsl_vec2_str = f"const vec2 SKY_VISIBILITY_BOUNDS[{num_angles}] = vec2[](\n"
glsl_vec2_str += ",\n".join([f"    vec2({m[0]:.10f}, {m[1]:.10f})" for m in mins_maxes])
glsl_vec2_str += "\n);"
print(glsl_vec2_str)

print("\n")

# Print float array
glsl_float_str = f"const float SKY_VISIBILITY_DIRECTION_DISTRIBUTIONS[{num_angles+1}] = float[](\n"
glsl_float_str += ",\n".join([f"    {l:.10f}" for l in ([0.0]+lengths)])
glsl_float_str += "\n);"
print(glsl_float_str)
