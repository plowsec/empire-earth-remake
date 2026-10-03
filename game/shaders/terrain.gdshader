// Terrain: 8 PBR layers from Texture2DArrays, height-blended by two control maps,
// anti-tiling (dual-scale + rotation), slope-driven cliffs, fog of war.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2DArray albedo_tex : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2DArray normal_tex : filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D control_a : filter_linear, repeat_disable;   // layers 0..3
uniform sampler2D control_b : filter_linear, repeat_disable;   // layers 4..7
uniform sampler2D fog_tex : filter_linear, repeat_disable;     // r: visible, g: explored
uniform sampler2D macro_noise : filter_linear_mipmap, repeat_enable;
uniform vec2 map_size = vec2(800.0);
uniform float uv_scale = 0.11;       // texture repeats per meter
uniform float blend_sharpness = 6.0;
uniform float cliff_start = 0.30;
uniform float cliff_end = 0.55;
uniform vec4 layer_tint[8];
uniform float fog_enabled = 1.0;

varying vec3 v_world;
varying vec3 v_wnormal;

void vertex() {
	v_world = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	v_wnormal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

vec2 rot(vec2 p, float a) {
	float c = cos(a), s = sin(a);
	return vec2(c * p.x - s * p.y, s * p.x + c * p.y);
}

// Sample one layer at two scales, blended by macro noise to hide tiling.
void sample_layer(int i, vec2 wp, float mn, out vec4 alb, out vec4 nrm) {
	float fi = float(i);
	vec2 uv1 = rot(wp * uv_scale, fi * 1.7);
	vec2 uv2 = rot(wp * uv_scale * 0.37, fi * 2.9 + 0.7) + vec2(0.31, 0.77);
	vec4 a1 = texture(albedo_tex, vec3(uv1, fi));
	vec4 a2 = texture(albedo_tex, vec3(uv2, fi));
	vec4 n1 = texture(normal_tex, vec3(uv1, fi));
	vec4 n2 = texture(normal_tex, vec3(uv2, fi));
	float t = smoothstep(0.35, 0.65, mn);
	alb = mix(a1, a2, t * 0.6);
	nrm = mix(n1, n2, t * 0.6);
	alb.rgb *= layer_tint[i].rgb;
}

void fragment() {
	vec2 muv = v_world.xz / map_size;
	vec4 ca = texture(control_a, muv);
	vec4 cb = texture(control_b, muv);
	float w[8] = float[8](ca.r, ca.g, ca.b, ca.a, cb.r, cb.g, cb.b, cb.a);

	// steep slopes become rock (layer 6) regardless of the control map
	float slope = 1.0 - clamp(v_wnormal.y, 0.0, 1.0);
	float cliff = smoothstep(cliff_start, cliff_end, slope);
	float underwater = smoothstep(0.2, -1.6, v_world.y);
	for (int i = 0; i < 8; i++) {
		w[i] *= (1.0 - cliff);
	}
	w[6] += cliff;
	// below sea level: blend toward seabed
	for (int i = 0; i < 8; i++) {
		w[i] *= (1.0 - underwater);
	}
	w[7] += underwater;

	float mn = texture(macro_noise, v_world.xz * 0.004).r;
	float mn2 = texture(macro_noise, v_world.xz * 0.019 + vec2(0.5)).r;

	vec4 albs[8];
	vec4 nrms[8];
	float total = 0.0;
	float maxh = -10.0;
	float hw[8];
	for (int i = 0; i < 8; i++) {
		hw[i] = 0.0;
		if (w[i] > 0.004) {
			sample_layer(i, v_world.xz, mn, albs[i], nrms[i]);
			float h = albs[i].a + w[i] * 1.6;
			hw[i] = h;
			maxh = max(maxh, h);
		}
	}
	vec3 col = vec3(0.0);
	vec2 nxy = vec2(0.0);
	float rough = 0.0;
	for (int i = 0; i < 8; i++) {
		if (w[i] > 0.004) {
			float b = max(hw[i] - maxh + 1.0 / blend_sharpness, 0.0) * w[i];
			col += albs[i].rgb * b;
			nxy += (nrms[i].rg * 2.0 - 1.0) * b;
			rough += nrms[i].b * b;
			total += b;
		}
	}
	col /= max(total, 1e-4);
	nxy /= max(total, 1e-4);
	rough /= max(total, 1e-4);

	// large-scale color variation so fields don't look uniform
	col *= mix(0.8, 1.08, mn2);
	// worn ground: dry, desaturated patches break up the uniform fields
	float dry = smoothstep(0.35, 0.7, texture(macro_noise, v_world.xz * 0.031).r);
	float cl = dot(col, vec3(0.299, 0.587, 0.114));
	col = mix(col, mix(vec3(cl), col, 0.55) * vec3(1.06, 1.0, 0.9), dry * 0.35);
	col = mix(col, col * vec3(1.05, 1.0, 0.86), smoothstep(0.55, 0.8, mn) * 0.5);

	// wet sand darkening near the waterline
	float wet = smoothstep(1.2, 0.1, v_world.y) * (1.0 - underwater);
	col *= mix(1.0, 0.62, wet);
	rough = mix(rough, 0.25, wet * 0.8);

	// fog of war
	vec2 fog = texture(fog_tex, muv).rg;
	float vis = mix(1.0, fog.r, fog_enabled);
	float expl = mix(1.0, fog.g, fog_enabled);
	float lum = dot(col, vec3(0.299, 0.587, 0.114));
	vec3 fogged = mix(vec3(lum), col, 0.35) * 0.42;
	col = mix(fogged, col, vis);
	col *= expl;

	ALBEDO = col;
	NORMAL_MAP = vec3(nxy * 0.5 + 0.5, 1.0);
	NORMAL_MAP_DEPTH = 1.2;
	ROUGHNESS = clamp(rough, 0.05, 1.0);
	SPECULAR = 0.35 * expl;
}
