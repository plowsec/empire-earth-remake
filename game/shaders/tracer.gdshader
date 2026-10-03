// Additive streak for bullets/tracers. INSTANCE_CUSTOM.x = life 0..1, COLOR = tint.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled;
varying vec4 v_col;
varying float v_life;
void vertex() { v_col = COLOR; v_life = INSTANCE_CUSTOM.x; }
void fragment() {
	float edge = 1.0 - abs(UV.y * 2.0 - 1.0);
	float along = smoothstep(0.0, 0.3, UV.x) * (0.4 + 0.6 * UV.x);
	ALBEDO = v_col.rgb * 4.0;
	ALPHA = edge * edge * along * v_life * v_col.a;
}
