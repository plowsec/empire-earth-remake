// Selection ring decal (flat quad), per-instance color.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled;
varying vec4 v_col;
void vertex() { v_col = COLOR; }
void fragment() {
	vec2 p = UV * 2.0 - 1.0;
	float r = length(p);
	float ring = smoothstep(0.76, 0.84, r) * (1.0 - smoothstep(0.95, 1.0, r));
	// rotating dashes make selection readable even in crowds
	float ang = atan(p.y, p.x);
	float dash = 0.75 + 0.25 * step(0.0, sin(ang * 8.0 + TIME * 2.0));
	float glow = (1.0 - smoothstep(0.55, 1.0, r)) * 0.18;
	ring *= dash * 1.6;
	ALBEDO = v_col.rgb;
	ALPHA = (ring + glow) * v_col.a;
}
