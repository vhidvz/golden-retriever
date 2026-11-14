// The coat of the skinned dog: the whole body drawn in layers ("shells"), each
// pushed a little further out, with strands thinning towards their tips. The
// coat lies back along the body and hangs under gravity; its length and shade
// vary over the body and come from the mesh (vertex colour).
shader_type spatial;
render_mode cull_back;

uniform float coat_scale = 1.0;             // puppies are fluffier
uniform float strand = 0.0045;              // width of one strand cell
uniform float lay = 0.75;                   // 0 = standing on end, 1 = lying flat
uniform vec3 root_color : source_color = vec3(0.62, 0.40, 0.16);
uniform vec3 tip_color : source_color = vec3(0.93, 0.74, 0.42);

varying float shell;    // 0 at the skin, 1 at the tips
varying vec3 shade;

float hash(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

void vertex() {
	shell = UV2.x;
	shade = COLOR.rgb;
	float length = COLOR.a * 0.1 * coat_scale;
	// Backwards and down, projected onto the surface: the way a coat is brushed.
	vec3 flow = vec3(-0.75, -0.65, 0.0);
	vec3 along = flow - NORMAL * dot(flow, NORMAL);
	VERTEX += normalize(NORMAL + along * lay * 1.6) * length * shell;
}

void fragment() {
	vec2 uv = UV / strand;
	float one = hash(floor(uv));
	float reach = 0.35 + 0.65 * one;            // strands differ in length
	float from_centre = length(fract(uv) - 0.5) * 2.0;
	if (shell > 0.0) {
		float along = shell / reach;
		if (along > 1.0 || 1.0 - along < from_centre) {
			discard;
		}
	}
	ALBEDO = mix(root_color, tip_color, one) * shade * mix(0.40, 1.0, shell);
	ROUGHNESS = 0.85;
	SPECULAR = 0.2;
}
