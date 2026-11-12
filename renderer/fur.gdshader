// Shell fur: the mesh is drawn once per shell (MultiMesh instance), each pushed a
// little further along the normal, and each strand thins out towards its tip.
shader_type spatial;
render_mode cull_back;

uniform float fur_length = 0.035;
uniform vec2 density = vec2(300.0, 150.0);
uniform vec3 comb = vec3(-0.8, -0.6, 0.0);   // the way the coat lies, in the part's own space
uniform float lay = 0.6;                     // 0 = standing on end, 1 = lying along `comb`
uniform vec3 root_color : source_color = vec3(0.62, 0.40, 0.16);
uniform vec3 tip_color : source_color = vec3(0.93, 0.74, 0.42);

varying float shell; // 0 at the skin, 1 at the fur tips

float hash(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

void vertex() {
	shell = INSTANCE_CUSTOM.x;
	// Longer strands hang: bend the outer shells towards the ground.
	vec3 down = normalize((inverse(MODEL_MATRIX) * vec4(0.0, -1.0, 0.0, 0.0)).xyz);
	vec3 along = normalize(NORMAL + normalize(comb) * lay * 1.2);
	VERTEX += along * fur_length * shell + down * fur_length * 0.35 * shell * shell;
}

void fragment() {
	vec2 uv = UV * density;
	float strand = hash(floor(uv));
	float reach = 0.35 + 0.65 * strand;         // strands differ in length
	float from_centre = length(fract(uv) - 0.5) * 2.0;
	if (shell > 0.0) {
		float along = shell / reach;
		if (along > 1.0 || 1.0 - along < from_centre) {
			discard;
		}
	}
	ALBEDO = mix(root_color, tip_color, strand) * mix(0.38, 1.0, shell); // darker near the skin
	ROUGHNESS = 0.85;
	SPECULAR = 0.2;
}
