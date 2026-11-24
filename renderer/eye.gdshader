// A wet dog eye: a dark limbal ring, a brown iris with radial streaks, a round
// pupil that follows the look direction, and a small specular catch.
shader_type spatial;
render_mode cull_back;

uniform vec3 iris_color : source_color = vec3(0.24, 0.13, 0.05);
uniform vec3 iris_light : source_color = vec3(0.46, 0.27, 0.10);
uniform vec3 pupil_dir = vec3(1.0, 0.0, 0.0);

varying vec3 local_dir;

void vertex() {
    local_dir = normalize(VERTEX);
}

void fragment() {
    vec3 d = normalize(local_dir);
    vec3 look = normalize(pupil_dir);
    float front = clamp(dot(d, look), -1.0, 1.0);
    float radial = sqrt(max(1.0 - front * front, 0.0));
    float pupil = 1.0 - smoothstep(0.085, 0.135, radial);
    float iris = smoothstep(0.10, 0.17, radial) * (1.0 - smoothstep(0.54, 0.64, radial));
    float limbal = smoothstep(0.50, 0.57, radial) * (1.0 - smoothstep(0.61, 0.67, radial));
    float angle = atan(d.y, d.z);
    float streak = 0.5 + 0.5 * sin(angle * 23.0 + sin(angle * 7.0) * 2.2 + radial * 34.0);
    vec3 tone = mix(iris_color, iris_light, streak * 0.75 + 0.15);
    vec3 colour = mix(iris_color * 0.35, tone, iris);
    colour = mix(colour, vec3(0.008, 0.006, 0.005), pupil);
    colour = mix(colour, vec3(0.018, 0.012, 0.009), limbal * 0.9);
    ALBEDO = colour;
    ROUGHNESS = mix(0.42, 0.12, iris);
    SPECULAR = 0.75;
    METALLIC = 0.0;
}
