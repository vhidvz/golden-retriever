// The coat: the body drawn again in soft layers lifted off the skin. See coat.gdshaderinc.
shader_type spatial;
render_mode unshaded, cull_back, blend_mix, depth_draw_never;
#define SHELLS
#include "res://coat.gdshaderinc"
