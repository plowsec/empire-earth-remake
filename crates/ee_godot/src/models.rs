//! Model registry: loads `res://assets/models/<key>.glb`, splits it into parts
//! (body / turret / rotor ...) and converts materials to the unit uber-shader.
//! Missing models fall back to procedural placeholders so the game always runs.
use crate::terrain::TILE;
use ee_sim::defs::{Class, Def};
use godot::classes::mesh::PrimitiveType;
use godot::classes::{
    ArrayMesh, BaseMaterial3D, BoxMesh, CapsuleMesh, CylinderMesh, Material, Mesh, MeshInstance3D, Node, Node3D,
    PackedScene, PrismMesh, ResourceLoader, Shader, ShaderMaterial, SphereMesh, StandardMaterial3D, Texture2D,
};
use godot::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Body,
    /// yaws toward the current target around its pivot
    Turret,
    /// spins around its local Y axis
    Rotor,
    /// spins around its local X axis (tail rotors, propellers)
    RotorX,
}

pub struct Part {
    pub mesh: Gd<Mesh>,
    /// transform relative to the model root
    pub local: Transform3D,
    pub role: Role,
    /// rotation pivot (model space) for turrets/rotors
    pub pivot: Vector3,
    /// inverse of the part's rest yaw (turrets aim relative to model forward)
    pub rest_inv: Basis,
}

pub struct Model {
    pub parts: Vec<Part>,
    /// visual scale applied per instance (infantry are exaggerated for readability)
    pub scale: f32,
    pub height: f32,
    /// selection ring radius in meters
    pub radius: f32,
    /// far-distance version (no shadows)
    pub lod1: Option<usize>,
    pub shadows: bool,
}

pub struct Models {
    surf_albedo: Option<Gd<godot::classes::TextureLayered>>,
    surf_normal: Option<Gd<godot::classes::TextureLayered>>,
    pub list: Vec<Model>,
    pub by_def: Vec<usize>,
    by_name: HashMap<String, usize>,
    unit_shader: Gd<Shader>,
    noise: Option<Gd<Texture2D>>,
    fog: Option<(Gd<Texture2D>, Vector2)>,
}

/// Material name -> (surface kind, mode, repeats per meter, strength).
/// Kinds index the packed surface array: concrete, brick, roof, corrugated,
/// plaster, wood, paint, fabric, asphalt, rock, bark.
fn surface_for(name: &str, def: &Def) -> Option<(i32, i32, f32, f32)> {
    let base = name.split('.').next().unwrap_or(name);
    let soft = matches!(def.class(), Class::Citizen | Class::Infantry);
    Some(match base {
        "concrete" | "concrete_dark" | "stone_light" => (0, 0, 0.35, 0.9),
        "brick" => (1, 1, 0.45, 1.0),
        "roof_red" => (2, 1, 0.4, 1.0),
        "roof_gray" | "metal_sheet" => (3, 0, 0.5, 0.9),
        "white" | "marble" | "medic_white" => (4, 0, 0.35, 0.8),
        "wood" | "wood_dark" | "deck" | "crate" => (5, 1, 0.6, 0.9),
        "asphalt" => (8, 1, 0.3, 1.0),
        "rock" | "rock_dark" | "iron_ore" => (9, 0, 0.35, 1.0),
        "bark" | "bark_palm" => (10, 1, 0.8, 0.9),
        "khaki" | "canvas" => (7, 0, 2.5, 0.6),
        "team" if soft => (7, 0, 2.5, 0.6),
        "olive" | "olive_dark" if soft => (7, 0, 2.5, 0.6),
        "olive" | "olive_dark" | "tan" | "navy_gray" | "navy_dark" | "air_gray" | "air_dark" | "team" | "team_metal"
        | "hull_red" | "yellow_paint" | "gunmetal" | "steel" => (6, 0, 0.6, 0.55),
        _ => return None,
    })
}

pub fn visual_scale(def: &Def) -> f32 {
    match def.class() {
        Class::Citizen | Class::Infantry => 1.48,
        Class::Vehicle => 0.88,
        Class::Aircraft => match def.data.key.as_str() {
            "bomber" => 0.72,
            "nuke_bomber" => 0.78,
            "helicopter" => 0.95,
            _ => 0.9,
        },
        _ => 1.0,
    }
}

fn anim_mode_for(def: &Def) -> i32 {
    match def.class() {
        Class::Citizen | Class::Infantry => 1,
        Class::Resource if def.data.key == "tree" || def.data.key == "berries" => 2,
        _ => 0,
    }
}

impl Models {
    pub fn new(noise: Option<Gd<Texture2D>>, fog: Option<(Gd<Texture2D>, Vector2)>) -> Models {
        let unit_shader = godot::tools::load::<Shader>("res://shaders/unit.gdshader");
        let load_arr = |p: &str| -> Option<Gd<godot::classes::TextureLayered>> {
            godot::tools::try_load::<godot::classes::CompressedTexture2DArray>(p).ok().map(|t| t.upcast())
        };
        Models {
            surf_albedo: load_arr("res://assets/textures/surface_albedo_array.png"),
            surf_normal: load_arr("res://assets/textures/surface_normal_array.png"),
            list: Vec::new(),
            by_def: Vec::new(),
            by_name: HashMap::new(),
            unit_shader,
            noise,
            fog,
        }
    }

    pub fn load_all(&mut self, defs: &[Def]) {
        for d in defs {
            let name = d.data.model.clone();
            let idx = if let Some(&i) = self.by_name.get(&name) {
                i
            } else {
                let m = self.load_model(&name, d).unwrap_or_else(|| self.placeholder(d));
                self.list.push(m);
                let i = self.list.len() - 1;
                self.by_name.insert(name.clone(), i);
                if let Some(mut lod) = self.load_model(&format!("{name}_lod1"), d) {
                    lod.shadows = false;
                    // keep the full model's bounds for selection rings/bars
                    lod.radius = self.list[i].radius;
                    lod.height = self.list[i].height;
                    self.list.push(lod);
                    let li = self.list.len() - 1;
                    self.list[i].lod1 = Some(li);
                }
                i
            };
            self.by_def.push(idx);
        }
    }

    /// Load an extra named model (variants such as `tree_palm`).
    pub fn variant(&mut self, name: &str, like: &Def) -> usize {
        if let Some(&i) = self.by_name.get(name) {
            return i;
        }
        let m = self.load_model(name, like).unwrap_or_else(|| self.placeholder(like));
        self.list.push(m);
        let i = self.list.len() - 1;
        self.by_name.insert(name.to_string(), i);
        i
    }

    fn make_material(&self, src: Option<Gd<Material>>, def: &Def, height: f32) -> Gd<ShaderMaterial> {
        let mut m = ShaderMaterial::new_gd();
        m.set_shader(&self.unit_shader);
        m.set_shader_parameter("anim_mode", &anim_mode_for(def).to_variant());
        m.set_shader_parameter("model_height", &height.to_variant());
        if let Some(n) = &self.noise {
            m.set_shader_parameter("detail_noise", &n.to_variant());
        }
        let wear = match def.class() {
            Class::Building => 0.62,
            Class::Vehicle | Class::Ship => 0.3,
            Class::Resource => 0.0,
            _ => 0.12,
        };
        m.set_shader_parameter("wear", &(wear as f32).to_variant());
        let (team_sat, weathering) = match def.class() {
            Class::Building => (0.62f32, 1.0f32),
            Class::Vehicle | Class::Ship | Class::Aircraft => (0.72, 0.35),
            _ => (0.85, 0.0),
        };
        m.set_shader_parameter("team_sat", &team_sat.to_variant());
        let band = match def.class() {
            Class::Citizen | Class::Infantry => Vector2::new(0.66, 0.76),
            Class::Vehicle => Vector2::new(0.42, 0.52),
            Class::Ship => Vector2::new(0.3, 0.38),
            Class::Aircraft => Vector2::new(0.45, 0.6),
            _ => Vector2::ZERO,
        };
        m.set_shader_parameter("team_band", &band.to_variant());
        m.set_shader_parameter("weathering", &weathering.to_variant());
        if def.is_resource() {
            if let Some((tex, size)) = &self.fog {
                m.set_shader_parameter("use_world_fog", &true.to_variant());
                m.set_shader_parameter("fog_tex", &tex.to_variant());
                m.set_shader_parameter("map_size", &size.to_variant());
            }
            m.set_shader_parameter("tint_by_color", &1.0f32.to_variant());
        }
        if let (Some(a), Some(n)) = (&self.surf_albedo, &self.surf_normal) {
            m.set_shader_parameter("surface_albedo", &a.to_variant());
            m.set_shader_parameter("surface_normal", &n.to_variant());
        }
        if let Some(src) = src {
            let name = src.get_name().to_string().to_lowercase();
            if let Some((kind, mode, scale, strength)) = surface_for(&name, def) {
                m.set_shader_parameter("surface_kind", &kind.to_variant());
                m.set_shader_parameter("surface_mode", &mode.to_variant());
                m.set_shader_parameter("surface_scale", &scale.to_variant());
                m.set_shader_parameter("surface_strength", &strength.to_variant());
            }
            if name.starts_with("team") || name.contains("_team") {
                m.set_shader_parameter("team_mask", &1.0f32.to_variant());
            }
            if let Ok(sm) = src.try_cast::<BaseMaterial3D>() {
                m.set_shader_parameter("albedo", &sm.get_albedo().to_variant());
                m.set_shader_parameter("roughness", &sm.get_roughness().to_variant());
                m.set_shader_parameter("metallic", &sm.get_metallic().to_variant());
                if let Some(t) = sm.get_texture(godot::classes::base_material_3d::TextureParam::ALBEDO) {
                    m.set_shader_parameter("albedo_tex", &t.to_variant());
                    m.set_shader_parameter("use_tex", &true.to_variant());
                }
                if sm.get_feature(godot::classes::base_material_3d::Feature::EMISSION) {
                    m.set_shader_parameter("emission", &sm.get_emission().to_variant());
                    m.set_shader_parameter("emission_energy", &sm.get_emission_energy_multiplier().to_variant());
                }
            }
        }
        m
    }

    /// Copy a mesh's surfaces into a new ArrayMesh with converted materials.
    fn convert_mesh(&self, mesh: &Gd<Mesh>, overrides: &[Option<Gd<Material>>], def: &Def, height: f32) -> Gd<Mesh> {
        let mut out = ArrayMesh::new_gd();
        for s in 0..mesh.get_surface_count() {
            let arrays = mesh.surface_get_arrays(s);
            out.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
            let src = overrides.get(s as usize).cloned().flatten().or_else(|| mesh.surface_get_material(s));
            let mat = self.make_material(src, def, height);
            out.surface_set_material(s, &mat);
        }
        out.upcast()
    }

    fn load_model(&self, name: &str, def: &Def) -> Option<Model> {
        let path = format!("res://assets/models/{name}.glb");
        let mut rl = ResourceLoader::singleton();
        if !rl.exists(&path) {
            return None;
        }
        let scene = rl.load(&path)?.try_cast::<PackedScene>().ok()?;
        let root = scene.instantiate()?;
        let mut raw: Vec<(Gd<Mesh>, Vec<Option<Gd<Material>>>, Transform3D, Role, Vector3)> = Vec::new();
        let mut aabb_min = Vector3::splat(f32::MAX);
        let mut aabb_max = Vector3::splat(f32::MIN);
        collect(&root, Transform3D::IDENTITY, Role::Body, Vector3::ZERO, &mut raw);
        for (mesh, _, xf, _, _) in &raw {
            let a = mesh.get_aabb();
            for corner in 0..8 {
                let p = Vector3::new(
                    if corner & 1 == 0 { a.position.x } else { a.position.x + a.size.x },
                    if corner & 2 == 0 { a.position.y } else { a.position.y + a.size.y },
                    if corner & 4 == 0 { a.position.z } else { a.position.z + a.size.z },
                );
                let w = *xf * p;
                aabb_min = aabb_min.coord_min(w);
                aabb_max = aabb_max.coord_max(w);
            }
        }
        let height = (aabb_max.y - aabb_min.y.min(0.0)).max(0.2);
        let parts = raw
            .into_iter()
            .map(|(mesh, ov, local, role, pivot)| {
                let f = local.basis.col_c();
                let rest_yaw = f.x.atan2(f.z);
                Part { mesh: self.convert_mesh(&mesh, &ov, def, height), local, role, pivot, rest_inv: Basis::from_axis_angle(Vector3::UP, -rest_yaw) }
            })
            .collect();
        let ext = (aabb_max - aabb_min).abs();
        let radius = (ext.x.max(ext.z) * 0.55).max(0.5);
        let mut root = root;
        root.queue_free();
        let scale = visual_scale(def);
        Some(Model { parts, height: height * scale, radius: radius * scale, scale, lod1: None, shadows: true })
    }

    fn placeholder(&self, def: &Def) -> Model {
        let key = def.data.key.as_str();
        let (sw, sh) = def.size();
        let r = def.radius.to_f32() * TILE;
        let mut parts: Vec<(Gd<Mesh>, Transform3D, Role, Color, bool)> = Vec::new();
        let team = Color::from_rgb(1.0, 1.0, 1.0);
        let olive = Color::from_rgb(0.33, 0.36, 0.24);
        let at = |x: f32, y: f32, z: f32| Transform3D::new(Basis::IDENTITY, Vector3::new(x, y, z));
        match def.class() {
            Class::Citizen | Class::Infantry => {
                let mut c = CapsuleMesh::new_gd();
                c.set_radius(0.28);
                c.set_height(1.8);
                parts.push((c.upcast(), at(0.0, 0.9, 0.0), Role::Body, team, true));
            }
            Class::Vehicle => {
                let mut b = BoxMesh::new_gd();
                b.set_size(Vector3::new(r * 1.1, r * 0.5, r * 1.9));
                parts.push((b.upcast(), at(0.0, r * 0.35, 0.0), Role::Body, olive, false));
                let mut t = BoxMesh::new_gd();
                t.set_size(Vector3::new(r * 0.7, r * 0.3, r * 0.8));
                parts.push((t.upcast(), at(0.0, r * 0.75, 0.0), Role::Turret, team, true));
            }
            Class::Aircraft => {
                let mut b = PrismMesh::new_gd();
                b.set_size(Vector3::new(r * 2.0, r * 0.25, r * 1.6));
                parts.push((b.upcast(), Transform3D::new(Basis::from_euler(EulerOrder::YXZ, Vector3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0)), Vector3::ZERO), Role::Body, team, true));
            }
            Class::Ship => {
                let mut b = BoxMesh::new_gd();
                b.set_size(Vector3::new(r * 0.7, r * 0.5, r * 2.0));
                parts.push((b.upcast(), at(0.0, 0.2, 0.0), Role::Body, Color::from_rgb(0.45, 0.48, 0.5), false));
                let mut t = BoxMesh::new_gd();
                t.set_size(Vector3::new(r * 0.4, r * 0.5, r * 0.6));
                parts.push((t.upcast(), at(0.0, r * 0.6, 0.0), Role::Body, team, true));
            }
            Class::Building => {
                let hgt = match key {
                    "capitol" => 9.0,
                    "farm" => 0.3,
                    "house" => 4.0,
                    "guard_tower" | "aa_site" => 7.0,
                    _ => 5.0,
                };
                let mut b = BoxMesh::new_gd();
                b.set_size(Vector3::new(sw as f32 * TILE * 0.88, hgt, sh as f32 * TILE * 0.88));
                let col = if key == "farm" { Color::from_rgb(0.75, 0.62, 0.25) } else { Color::from_rgb(0.72, 0.70, 0.64) };
                parts.push((b.upcast(), at(0.0, hgt * 0.5, 0.0), Role::Body, col, false));
                if key != "farm" {
                    let mut roof = BoxMesh::new_gd();
                    roof.set_size(Vector3::new(sw as f32 * TILE * 0.5, 0.6, sh as f32 * TILE * 0.5));
                    parts.push((roof.upcast(), at(0.0, hgt + 0.3, 0.0), Role::Body, team, true));
                }
            }
            Class::Resource => match key {
                "tree" => {
                    let mut trunk = CylinderMesh::new_gd();
                    trunk.set_top_radius(0.18);
                    trunk.set_bottom_radius(0.25);
                    trunk.set_height(2.0);
                    parts.push((trunk.upcast(), at(0.0, 1.0, 0.0), Role::Body, Color::from_rgb(0.35, 0.24, 0.15), false));
                    let mut crown = CylinderMesh::new_gd();
                    crown.set_top_radius(0.0);
                    crown.set_bottom_radius(1.6);
                    crown.set_height(4.5);
                    parts.push((crown.upcast(), at(0.0, 4.0, 0.0), Role::Body, Color::from_rgb(0.16, 0.36, 0.14), false));
                }
                "berries" => {
                    let mut s = SphereMesh::new_gd();
                    s.set_radius(0.9);
                    s.set_height(1.3);
                    parts.push((s.upcast(), at(0.0, 0.5, 0.0), Role::Body, Color::from_rgb(0.2, 0.42, 0.18), false));
                }
                "fish" => {
                    let mut s = SphereMesh::new_gd();
                    s.set_radius(0.01);
                    s.set_height(0.02);
                    parts.push((s.upcast(), at(0.0, -1.0, 0.0), Role::Body, Color::from_rgb(0.2, 0.42, 0.18), false));
                }
                _ => {
                    let col = match key {
                        "gold_mine" => Color::from_rgb(0.85, 0.68, 0.2),
                        "iron_mine" => Color::from_rgb(0.45, 0.3, 0.25),
                        _ => Color::from_rgb(0.62, 0.6, 0.56),
                    };
                    let mut s = SphereMesh::new_gd();
                    s.set_radius(sw as f32 * TILE * 0.42);
                    s.set_height(sw as f32 * TILE * 0.45);
                    parts.push((s.upcast(), at(0.0, 0.3, 0.0), Role::Body, col, false));
                }
            },
        }
        let mut height: f32 = 0.5;
        for (m, xf, _, _, _) in &parts {
            let a = m.get_aabb();
            height = height.max(xf.origin.y + a.position.y + a.size.y);
        }
        let parts = parts
            .into_iter()
            .map(|(m, xf, role, col, is_team)| {
                let mut sm = StandardMaterial3D::new_gd();
                sm.set_albedo(col);
                sm.set_roughness(0.7);
                if is_team {
                    sm.set_name("team");
                }
                let ov = vec![Some(sm.upcast::<Material>())];
                Part { mesh: self.convert_mesh(&m, &ov, def, height), local: xf, role, pivot: xf.origin, rest_inv: Basis::IDENTITY }
            })
            .collect();
        let radius = if def.is_building() { sw.max(sh) as f32 * TILE * 0.6 } else { (r * 1.3).max(0.6) };
        Model { parts, height, radius, scale: 1.0, lod1: None, shadows: true }
    }
}

fn collect(
    node: &Gd<Node>,
    parent: Transform3D,
    role: Role,
    pivot: Vector3,
    out: &mut Vec<(Gd<Mesh>, Vec<Option<Gd<Material>>>, Transform3D, Role, Vector3)>,
) {
    let mut xf = parent;
    let mut role = role;
    let mut pivot = pivot;
    let name = node.get_name().to_string().to_lowercase();
    if let Ok(n3) = node.clone().try_cast::<Node3D>() {
        xf = parent * n3.get_transform();
    }
    let new_role = if name.starts_with("turret") {
        Some(Role::Turret)
    } else if name.starts_with("rotor_tail") || name.starts_with("prop") {
        Some(Role::RotorX)
    } else if name.starts_with("rotor") {
        Some(Role::Rotor)
    } else {
        None
    };
    if let Some(r) = new_role {
        if role == Role::Body {
            role = r;
            pivot = xf.origin;
        }
    }
    if let Ok(mi) = node.clone().try_cast::<MeshInstance3D>() {
        if let Some(mesh) = mi.get_mesh() {
            let n = mesh.get_surface_count();
            let ov: Vec<Option<Gd<Material>>> = (0..n).map(|s| mi.get_surface_override_material(s)).collect();
            out.push((mesh, ov, xf, role, pivot));
        }
    }
    for c in node.get_children().iter_shared() {
        collect(&c, xf, role, pivot, out);
    }
}
