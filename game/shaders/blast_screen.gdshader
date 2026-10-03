shader_type canvas_item;
uniform sampler2D screen_tex : hint_screen_texture, filter_linear;
uniform vec2 center = vec2(0.5);
uniform float aspect = 1.7778;
uniform float flash = 0.0;
uniform float wave = 0.0;
uniform float strength = 1.0;
void fragment() {
    // Elliptical pressure front travels out from the explosion across the ground.
    vec2 d = (SCREEN_UV - center) * vec2(aspect, 1.9);
    float dist = length(d);
    float radius = wave * 1.7;
    float front = exp(-pow((dist - radius) / 0.027, 2.0));
    float live = smoothstep(0.0, 0.08, wave) * (1.0 - smoothstep(0.6, 1.0, wave));
    vec2 offset = normalize(d + vec2(0.0001)) * front * live * 0.018 * strength;
    vec3 scene = texture(screen_tex, clamp(SCREEN_UV + offset, vec2(0.001), vec2(0.999))).rgb;
    scene += vec3(0.28, 0.26, 0.21) * front * live * strength;
    // Thermal flash clips the visible world to white while keeping the HUD legible.
    vec3 glare = mix(vec3(1.0, 0.91, 0.72), vec3(1.0), smoothstep(0.35, 0.8, flash));
    COLOR = vec4(mix(scene, glare, clamp(flash * strength, 0.0, 1.0)), 1.0);
}
