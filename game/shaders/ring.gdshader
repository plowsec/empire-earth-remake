// Selection ring decal (flat quad), per-instance color.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled;
varying vec4 v_col;
void vertex() { v_col = COLOR; }
void fragment() {
	vec2 p = UV * 2.0 - 1.0;
	float r = length(p);
	float ring = smoothstep(0.80, 0.86, r) * (1.0 - smoothstep(0.94, 1.0, r));
	float glow = (1.0 - smoothstep(0.6, 1.0, r)) * 0.12;
	ALBEDO = v_col.rgb;
	ALPHA = (ring + glow) * v_col.a;
}
