// Buddy's coat and skin, shared by the two shaders that draw him:
//   skin.gdshader  the body itself, solid
//   coat.gdshader  the coat, as soft layers lifted off the body
//
// At the size he is on the screen a single hair is thinner than a pixel, so the
// coat is not drawn as strands. What the eye sees of a real dog at this size is
// locks of hair lying along the body, a silky sheen that follows them, darker
// depths between them and a soft, wispy outline; that is what is drawn here.
// Everything about the coat at a point comes from the mesh (tools/build_dog.py):
// its length, colour, the way it lies, and how much light reaches it.

uniform float coat_scale = 1.0;                      // puppies are fluffier
uniform float pale = 0.0;                            // … and paler
uniform vec3 sun = vec3(-0.37, 0.77, 0.53);          // towards the light, the same as the scene's lamp
uniform vec3 sun_color : source_color = vec3(1.0, 0.96, 0.88);
uniform vec3 sky_color : source_color = vec3(0.62, 0.66, 0.74);
uniform vec3 ground_color : source_color = vec3(0.34, 0.30, 0.27);

varying float shell;     // 0 at the skin, 1 at the tips of the coat
varying float coat;      // how long the coat is here
varying vec3 grain;      // where this point is along and around its part of the body
varying vec3 lie;        // the way the coat lies, as the camera sees it
varying vec2 bare;       // x: bare skin instead of coat, y: wet shine
varying float pressed;   // how far this point of the coat was pushed up to stay above the floor
varying float end_on;    // 1 where the surface faces down the length of its part, so the locks are seen from their ends

float hash3(vec3 p) {
	p = fract(p * 0.3183099 + vec3(0.71, 0.113, 0.419));
	p *= 17.0;
	return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float noise3(vec3 x) {
	vec3 i = floor(x);
	vec3 f = fract(x);
	f = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(mix(hash3(i), hash3(i + vec3(1.0, 0.0, 0.0)), f.x), mix(hash3(i + vec3(0.0, 1.0, 0.0)), hash3(i + vec3(1.0, 1.0, 0.0)), f.x), f.y),
		mix(mix(hash3(i + vec3(0.0, 0.0, 1.0)), hash3(i + vec3(1.0, 0.0, 1.0)), f.x), mix(hash3(i + vec3(0.0, 1.0, 1.0)), hash3(i + vec3(1.0, 1.0, 1.0)), f.x), f.y),
		f.z);
}

void vertex() {
	shell = UV2.x;
	coat = UV2.y * coat_scale;
	grain = CUSTOM0.xyz;
	end_on = CUSTOM0.w;
	bare = UV;
	vec3 flow = normalize(TANGENT);
	// A hair leaves the skin at an angle and then bends over along the body and down.
	vec3 down = (inverse(MODEL_MATRIX) * vec4(0.0, -1.0, 0.0, 0.0)).xyz;
	float over = mix(0.55, 0.95, shell);
	// Long hair hangs: the longer the coat, the more its far end simply falls.
	vec3 way = normalize(NORMAL * (1.0 - 0.75 * over) + flow * over + down * (0.25 + 7.0 * coat) * shell);
	VERTEX += way * coat * shell;
	// Nothing of him goes through the floor: a belly or a plume that reaches it rests on it.
	vec4 placed = MODEL_MATRIX * vec4(VERTEX, 1.0);
	float floor_at = 0.003 + 0.006 * shell;
	pressed = max(floor_at - placed.y, 0.0);
	if (placed.y < floor_at) {
		placed.y = floor_at;
		VERTEX = (inverse(MODEL_MATRIX) * placed).xyz;
	}
	lie = normalize((MODELVIEW_MATRIX * vec4(normalize(flow + NORMAL * 0.25), 0.0)).xyz);
}

void fragment() {
	// Locks: long streaks along the part, a centimetre or so across, and finer strands inside them.
	vec3 along = vec3(grain.x + shell * coat * 0.6, grain.y, grain.z);
	// A Golden's coat is wavy: the locks ripple gently instead of running dead straight.
	float wavy = smoothstep(0.012, 0.06, coat);
	along.yz += 0.0032 * wavy * sin(grain.x * (95.0 + 40.0 * sin(grain.x * 5.3 + grain.z * 9.0)) + 5.0 * vec2(sin(grain.y * 37.0 + grain.x * 11.0), cos(grain.z * 41.0 - grain.x * 7.0)));
	// On the body the locks lie along it, so they are long streaks. Feathering hangs away
	// from the part it grows on, so there the locks are seen side by side instead.
	float hanging = smoothstep(0.055, 0.12, coat);
	float lengthwise = mix(5.0, 70.0, hanging);
	float lock = 0.55 * noise3(along * vec3(lengthwise, 95.0, 95.0)) + 0.45 * noise3(along * vec3(lengthwise * 2.2, 230.0, 230.0) + 11.0);
	float fine = noise3(along * vec3(lengthwise * 5.0, 560.0, 560.0) + 5.0);
	float furred = 1.0 - bare.x;

	vec3 tone = COLOR.rgb;
	tone = mix(tone, vec3(0.94, 0.80, 0.56), 0.55 * pale * furred);       // a puppy's coat is a pale cream gold
	// Depth: the coat is darker towards the skin, and lighter where a lock stands proud.
	float thick = smoothstep(0.004, 0.030, coat);          // a short coat has no depth to speak of
	float depth = mix(mix(0.86, mix(0.60, 1.08, shell), thick), 1.0, bare.x);
	// A short coat (the face, the paws) shows almost no locks; a long one shows them clearly.
	float locks = furred * smoothstep(0.003, 0.035, coat) * (1.0 - smoothstep(0.45, 0.9, end_on));
	vec3 albedo = tone * mix(1.0, mix(0.86, 1.12, lock), locks) * mix(1.0, mix(0.93, 1.06, fine), furred) * depth;

	vec3 n = normalize(NORMAL);
	vec3 to_sun = normalize((VIEW_MATRIX * vec4(sun, 0.0)).xyz);
	vec3 up = normalize((VIEW_MATRIX * vec4(0.0, 1.0, 0.0, 0.0)).xyz);
	float open = mix(COLOR.a, 1.0, 0.35 * shell);
	// A coat scatters light round the form, so the dark side is never black.
	float wrap = mix(0.6, 0.15, bare.x);
	float lit = clamp((dot(n, to_sun) + wrap) / (1.0 + wrap), 0.0, 1.0);
	vec3 ambient = mix(ground_color, sky_color, dot(n, up) * 0.5 + 0.5);
	vec3 colour = albedo * (sun_color * lit * 1.15 * mix(0.55, 1.0, open) + ambient * 0.62 * open);

	vec3 half_way = normalize(to_sun + VIEW);
	// The sheen of hair: brightest where the light glances along the strands.
	vec3 strand = normalize(lie + n * (lock - 0.5) * 0.25);
	float across = dot(strand, half_way);
	float sheen = pow(max(1.0 - across * across, 0.0), 22.0) * lit * furred;
	colour += tone * sun_color * sheen * 0.34 * mix(0.6, 1.0, shell) * open;
	// Wet skin (the nose, the lips) has a small bright highlight instead.
	colour += sun_color * pow(max(dot(n, half_way), 0.0), 48.0) * 0.55 * bare.y;
	// Light caught in the fringe of the outline.
	float edge = pow(1.0 - max(dot(n, VIEW), 0.0), 3.0);
	colour += tone * sky_color * edge * 0.30 * furred * open;

	ALBEDO = colour;
#ifdef SHELLS
	// Each lock reaches its own height; a layer shows only the locks that reach it, fading out at their tips.
	float reach = mix(0.30, 1.05, lock) - 0.22 * fine;
	// Coat pressed flat against the floor is hidden under him, not spread out around him.
	ALPHA = smoothstep(-0.04, 0.22, reach - shell) * furred * step(0.0005, coat) * (1.0 - smoothstep(0.002, 0.012, pressed));
#endif
}
