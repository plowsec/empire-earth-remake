// Building placement preview.
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, cull_back, shadows_disabled;
uniform vec4 tint : source_color = vec4(0.3, 1.0, 0.4, 0.45);
void fragment() {
	float rim = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 2.0);
	ALBEDO = tint.rgb;
	ALPHA = tint.a * (0.5 + rim);
}
