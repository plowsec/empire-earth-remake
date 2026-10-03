// Ocean: depth-tinted, refracted, shore foam, two scrolling normal layers,
// small vertex waves, fog of war.
shader_type spatial;
render_mode blend_mix, depth_draw_always, cull_back, diffuse_burley, specular_schlick_ggx;

uniform sampler2D DEPTH_TEXTURE : hint_depth_texture, filter_linear_mipmap;
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;
uniform sampler2D normal_a : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D normal_b : hint_normal, filter_linear_mipmap, repeat_enable;
uniform sampler2D foam_noise : filter_linear_mipmap, repeat_enable;
uniform sampler2D fog_tex : filter_linear, repeat_disable;
uniform vec2 map_size = vec2(800.0);
uniform vec4 shallow_color : source_color = vec4(0.10, 0.62, 0.62, 1.0);
uniform vec4 deep_color : source_color = vec4(0.01, 0.10, 0.22, 1.0);
uniform vec4 foam_color : source_color = vec4(0.95, 0.98, 1.0, 1.0);
uniform float depth_fade = 7.0;
uniform float clarity = 2.5;
uniform float wave_height = 0.18;
uniform float fog_enabled = 1.0;

varying vec3 v_world;

float wave(vec2 p, vec2 dir, float freq, float speed, float t) {
	return sin(dot(p, dir) * freq + t * speed);
}

void vertex() {
	vec3 w = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	float t = TIME;
	float h = wave(w.xz, normalize(vec2(1.0, 0.3)), 0.11, 1.1, t) * 0.6
		+ wave(w.xz, normalize(vec2(-0.4, 1.0)), 0.23, 1.7, t) * 0.3
		+ wave(w.xz, normalize(vec2(0.7, -0.8)), 0.47, 2.3, t) * 0.12;
	VERTEX.y += h * wave_height;
	v_world = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	float t = TIME;
	vec2 uv1 = v_world.xz * 0.018 + vec2(t * 0.012, t * 0.007);
	vec2 uv2 = v_world.xz * 0.043 + vec2(-t * 0.009, t * 0.015);
	vec3 n1 = texture(normal_a, uv1).rgb;
	vec3 n2 = texture(normal_b, uv2).rgb;
	vec3 nm = normalize(mix(n1, n2, 0.5) * 2.0 - 1.0);

	// scene depth behind the water surface
	float raw = texture(DEPTH_TEXTURE, SCREEN_UV).r;
	vec4 ndc = vec4(SCREEN_UV * 2.0 - 1.0, raw, 1.0);
	vec4 view = INV_PROJECTION_MATRIX * ndc;
	view.xyz /= view.w;
	float scene_dist = -view.z;
	float surf_dist = -VERTEX.z;
	float depth = max(scene_dist - surf_dist, 0.0);

	// refraction (offset sample, then reject samples that land in front)
	vec2 off = nm.xy * 0.025 * clamp(depth * 0.3, 0.0, 1.0);
	vec3 refr = texture(SCREEN_TEXTURE, SCREEN_UV + off).rgb;
	float raw2 = texture(DEPTH_TEXTURE, SCREEN_UV + off).r;
	vec4 v2 = INV_PROJECTION_MATRIX * vec4((SCREEN_UV + off) * 2.0 - 1.0, raw2, 1.0);
	v2.xyz /= v2.w;
	if (-v2.z < surf_dist) {
		refr = texture(SCREEN_TEXTURE, SCREEN_UV).rgb;
	}

	float fade = clamp(depth / depth_fade, 0.0, 1.0);
	vec3 water = mix(shallow_color.rgb, deep_color.rgb, pow(fade, 0.6));
	float see = exp(-depth / clarity);
	vec3 col = mix(water, refr * mix(vec3(1.0), shallow_color.rgb * 1.4, 0.5), see * 0.85);

	// shore foam: bands that move with the waves
	float fn = texture(foam_noise, v_world.xz * 0.08 + vec2(t * 0.02, 0.0)).r;
	float fn2 = texture(foam_noise, v_world.xz * 0.21 - vec2(0.0, t * 0.03)).r;
	float band = smoothstep(0.0, 1.0, sin(depth * 4.5 - t * 1.6) * 0.5 + 0.5);
	float shore = 1.0 - smoothstep(0.0, 1.4, depth);
	float foam = shore * smoothstep(0.35, 0.75, fn * 0.6 + fn2 * 0.4 + band * 0.35);
	foam = max(foam, (1.0 - smoothstep(0.0, 0.25, depth)) * 0.9);
	// whitecaps out at sea
	foam += smoothstep(0.78, 0.92, fn2 * fn * 1.9) * 0.25 * fade;
	col = mix(col, foam_color.rgb, clamp(foam, 0.0, 1.0));

	// fog of war
	vec2 muv = v_world.xz / map_size;
	vec2 fog = texture(fog_tex, clamp(muv, vec2(0.0), vec2(1.0))).rg;
	bool outside = muv.x < 0.0 || muv.y < 0.0 || muv.x > 1.0 || muv.y > 1.0;
	float vis = outside ? 0.55 : mix(1.0, fog.r, fog_enabled);
	float expl = outside ? 0.85 : mix(1.0, fog.g, fog_enabled);
	col = mix(col * 0.45, col, vis) * expl;

	ALBEDO = col;
	NORMAL_MAP = nm * 0.5 + 0.5;
	NORMAL_MAP_DEPTH = 0.6;
	ROUGHNESS = mix(0.04, 0.6, clamp(foam, 0.0, 1.0));
	METALLIC = 0.0;
	SPECULAR = 0.6 * expl;
}
