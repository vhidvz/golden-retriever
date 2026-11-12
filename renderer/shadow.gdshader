// Soft contact shadow on the "ground", so he reads as standing on the screen edge.
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, cull_disabled;

uniform float strength = 0.42;

void fragment() {
	float d = length(UV - 0.5) * 2.0;
	ALBEDO = vec3(0.0);
	ALPHA = strength * smoothstep(1.0, 0.15, d);
}
