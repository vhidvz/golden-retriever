// Draws one picture of Buddy (sprites.gd): this frame blended into the next for smooth
// movement, the previous clip fading out underneath, mirrored when he faces left.
// The pictures arrive premultiplied (sprites.gd), so filtering and mixing them keeps clean edges.
shader_type canvas_item;
render_mode blend_premul_alpha;

uniform sampler2D cur : filter_linear;
uniform sampler2D nxt : filter_linear;
uniform sampler2D old : filter_linear;
uniform float blend = 0.0;       // how far from this frame to the next
uniform float fade = 0.0;        // weight of the previous clip
uniform float shown = 1.0;       // fading between the pictures and the 3D body
uniform bool flip = false;
uniform bool flip_old = false;

void fragment() {
	vec2 here = flip ? vec2(1.0 - UV.x, UV.y) : UV;
	vec2 there = flip_old ? vec2(1.0 - UV.x, UV.y) : UV;
	vec4 now = mix(texture(cur, here), texture(nxt, here), blend);
	vec4 before = texture(old, there);
	COLOR = mix(now, before, fade) * shown;
}
