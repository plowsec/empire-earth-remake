// Player-color disc under every unit: friend or foe at a glance.
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, cull_disabled, shadows_disabled;
varying vec4 v_col;
void vertex() { v_col = COLOR; }
void fragment() {
	vec2 p = UV * 2.0 - 1.0;
	float r = length(p);
	float fill = (1.0 - smoothstep(0.6, 0.98, r)) * 0.28;
	float edge = smoothstep(0.7, 0.82, r) * (1.0 - smoothstep(0.9, 1.0, r)) * 0.75;
	ALBEDO = v_col.rgb;
	ALPHA = (fill + edge) * v_col.a;
}
