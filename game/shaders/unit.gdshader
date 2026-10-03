// Uber-shader for all MultiMesh-rendered models (units, buildings, resources).
// Per-instance data:
//   COLOR            = player color (team-colored surfaces use it)
//   INSTANCE_CUSTOM  = (anim_time, action, progress, flags)
//     action: 0 idle 1 move 2 attack 3 gather 4 build 5 carry 6 landed 7 dying
//     progress: construction 0..1 (1 = complete)
//     flags: 1 selected, 2 hovered, 4 fogged (remembered building), 8 damaged
// Infantry animation is procedural skeletal: UV2.x holds the body part id
// (1 L-leg, 2 R-leg, 3 L-arm, 4 R-arm, 5 torso/head, 6 weapon).
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

uniform vec4 albedo : source_color = vec4(1.0);
uniform sampler2D albedo_tex : source_color, filter_linear_mipmap_anisotropic, repeat_enable;
uniform bool use_tex = false;
uniform sampler2D detail_noise : filter_linear_mipmap, repeat_enable;
uniform float roughness : hint_range(0.0, 1.0) = 0.6;
uniform float metallic : hint_range(0.0, 1.0) = 0.0;
uniform float team_mask : hint_range(0.0, 1.0) = 0.0;
uniform vec4 emission : source_color = vec4(0.0);
uniform float emission_energy = 0.0;
uniform int anim_mode = 0;          // 0 rigid, 1 infantry, 2 foliage sway, 3 water bob
uniform float model_height = 2.0;   // for construction clipping
uniform float wear = 0.25;          // grime amount

varying float v_part;
varying vec4 v_custom;
varying vec3 v_team;
varying vec3 v_local;
varying float v_height01;

mat3 rot_x(float a) {
	float c = cos(a), s = sin(a);
	return mat3(vec3(1, 0, 0), vec3(0, c, s), vec3(0, -s, c));
}
mat3 rot_z(float a) {
	float c = cos(a), s = sin(a);
	return mat3(vec3(c, s, 0), vec3(-s, c, 0), vec3(0, 0, 1));
}

void vertex() {
	v_custom = INSTANCE_CUSTOM;
	v_team = COLOR.rgb;
	v_part = UV2.x;
	v_local = VERTEX;
	v_height01 = clamp(VERTEX.y / max(model_height, 0.01), 0.0, 1.0);
	float t = INSTANCE_CUSTOM.x;
	float action = INSTANCE_CUSTOM.y;

	if (anim_mode == 1) {
		int part = int(round(UV2.x));
		bool moving = action > 0.5 && action < 1.5 || action > 4.5 && action < 5.5;
		float walk = moving ? 1.0 : 0.0;
		float cyc = t * 7.5;
		vec3 v = VERTEX;
		vec3 n = NORMAL;
		// whole-body bob while walking
		float bob = walk * abs(sin(cyc)) * 0.05;
		if (part == 1 || part == 2) {
			float side = part == 1 ? 1.0 : -1.0;
			float a = walk * sin(cyc) * 0.55 * side;
			vec3 piv = vec3(0.0, 0.92, 0.0);
			mat3 r = rot_x(a);
			v = r * (v - piv) + piv;
			n = r * n;
		} else if (part == 3 || part == 4 || part == 6) {
			float side = part == 3 ? -1.0 : 1.0;
			vec3 piv = vec3(0.0, 1.42, 0.0);
			float a = walk * sin(cyc) * 0.45 * side;
			if (part == 6) { a = walk * sin(cyc) * 0.12; }
			if (action > 1.5 && action < 2.5) {
				// attack: aim forward with a little recoil
				a = -1.25 + max(0.0, sin(t * 9.0)) * 0.12;
			} else if (action > 2.5 && action < 4.5) {
				// gather/build: overhead chopping swing
				a = -1.9 + (sin(t * 5.5) * 0.5 + 0.5) * 1.6;
			} else if (action > 4.5 && action < 5.5) {
				// carrying a load: arms forward
				a = -0.9;
			}
			mat3 r = rot_x(a);
			v = r * (v - piv) + piv;
			n = r * n;
		} else if (part == 5) {
			float lean = 0.0;
			if (action > 2.5 && action < 4.5) {
				lean = (sin(t * 5.5) * 0.5 + 0.5) * 0.35;
			}
			lean += walk * 0.08;
			vec3 piv = vec3(0.0, 0.95, 0.0);
			mat3 r = rot_x(lean);
			v = r * (v - piv) + piv;
			n = r * n;
		}
		v.y += bob;
		if (action > 6.5) {
			// dying: topple over backwards
			float k = clamp(t, 0.0, 1.0);
			mat3 r = rot_x(k * 1.5);
			v = r * v;
			n = r * n;
			v.y -= k * 0.2;
		}
		VERTEX = v;
		NORMAL = n;
	} else if (anim_mode == 2) {
		// foliage: sway increasing with height
		vec3 wp = (MODEL_MATRIX * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
		float ph = wp.x * 0.13 + wp.z * 0.17;
		float sway = sin(TIME * 1.3 + ph) * 0.5 + sin(TIME * 2.9 + ph * 1.7) * 0.25;
		float k = pow(max(VERTEX.y, 0.0) / max(model_height, 0.1), 1.6);
		VERTEX.x += sway * k * 0.10 * model_height;
		VERTEX.z += sway * k * 0.05 * model_height;
	}
}

void fragment() {
	vec3 base = albedo.rgb;
	if (use_tex) {
		base *= texture(albedo_tex, UV).rgb;
	}
	// team color: keep the surface's shading, replace hue
	float lum = dot(base, vec3(0.299, 0.587, 0.114));
	vec3 team = v_team * (0.35 + lum * 1.3);
	base = mix(base, team, team_mask);

	// procedural grime / wear: darker toward the ground, noisy streaks
	float n = texture(detail_noise, v_local.xz * 0.9 + v_local.yy * 0.35).r;
	float grime = (1.0 - smoothstep(0.0, 0.35, v_height01)) * 0.5 + (n - 0.5) * 0.6;
	base *= 1.0 - clamp(grime, 0.0, 1.0) * wear;

	float flags = v_custom.w;
	int fl = int(round(flags));
	float progress = v_custom.z;
	vec3 emit = emission.rgb * emission_energy;

	// construction: geometry rises from the ground; the unbuilt part is a blueprint
	if (progress < 0.999) {
		float cut = progress * 1.02;
		if (v_height01 > cut + 0.02) {
			// wireframe-ish blueprint lines above the build line
			vec3 g = fract(v_local * 2.0);
			float line = step(0.93, max(max(g.x, g.y), g.z));
			if (line < 0.5) {
				discard;
			}
			base = vec3(0.35, 0.65, 1.0);
			emit = vec3(0.2, 0.5, 1.0) * 1.2;
		} else if (v_height01 > cut - 0.03) {
			emit = vec3(1.0, 0.6, 0.2) * 2.0;
		}
	}
	// selection / hover rim light
	float rim = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 3.0);
	if ((fl & 2) != 0) {
		emit += vec3(0.6, 0.8, 1.0) * rim * 0.9;
	}
	if ((fl & 4) != 0) {
		// remembered (fogged) building
		base = mix(vec3(lum), base, 0.3) * 0.45;
	}
	ALBEDO = base;
	ROUGHNESS = roughness;
	METALLIC = metallic;
	EMISSION = emit;
}
