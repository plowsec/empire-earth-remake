// Health bar billboard. INSTANCE_CUSTOM.x = hp fraction, .y = secondary (progress) fraction.
shader_type spatial;
render_mode unshaded, blend_mix, depth_test_disabled, cull_disabled, shadows_disabled;
varying vec4 v_c;
void vertex() {
	v_c = INSTANCE_CUSTOM;
	// billboard: keep instance origin & scale, face the camera
	vec3 origin = MODEL_MATRIX[3].xyz;
	float sx = length(MODEL_MATRIX[0].xyz);
	vec3 right = normalize(INV_VIEW_MATRIX[0].xyz);
	vec3 up = normalize(INV_VIEW_MATRIX[1].xyz);
	vec3 wp = origin + right * VERTEX.x * sx + up * VERTEX.y * sx;
	POSITION = PROJECTION_MATRIX * VIEW_MATRIX * vec4(wp, 1.0);
}
void fragment() {
	float hp = v_c.x;
	vec2 uv = UV;
	float border = step(uv.x, 0.03) + step(0.97, uv.x) + step(uv.y, 0.12) + step(0.88, uv.y);
	vec3 col = mix(vec3(0.9, 0.15, 0.1), vec3(0.2, 0.95, 0.25), smoothstep(0.25, 0.6, hp));
	col = uv.x < hp ? col : vec3(0.08);
	if (v_c.y > 0.0 && uv.y < 0.35) { col = uv.x < v_c.y ? vec3(0.3, 0.7, 1.0) : vec3(0.05); }
	ALBEDO = border > 0.0 ? vec3(0.0) : col;
	ALPHA = 0.92;
}
