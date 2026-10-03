//! A growable MultiMesh batch: one draw call for every instance of a mesh.
use godot::classes::multi_mesh::TransformFormat;
use godot::classes::{Mesh, MultiMesh, MultiMeshInstance3D, Node, ShaderMaterial};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::prelude::*;

const STRIDE: usize = 12 + 4 + 4;

pub struct Batch {
    pub node: Gd<MultiMeshInstance3D>,
    mm: Gd<MultiMesh>,
    buf: Vec<f32>,
    count: usize,
    capacity: usize,
    last_visible: i32,
}

impl Batch {
    pub fn new(parent: &mut Gd<Node>, mesh: &Gd<Mesh>, shadows: bool, material: Option<&Gd<ShaderMaterial>>) -> Batch {
        let mut mm = MultiMesh::new_gd();
        mm.set_transform_format(TransformFormat::TRANSFORM_3D);
        mm.set_use_colors(true);
        mm.set_use_custom_data(true);
        mm.set_mesh(mesh);
        mm.set_custom_aabb(Aabb::new(Vector3::new(-2000.0, -200.0, -2000.0), Vector3::new(6000.0, 800.0, 6000.0)));
        let mut node = MultiMeshInstance3D::new_alloc();
        node.set_multimesh(&mm);
        node.set_cast_shadows_setting(if shadows { ShadowCastingSetting::ON } else { ShadowCastingSetting::OFF });
        if let Some(m) = material {
            node.set_material_override(m);
        }
        parent.add_child(&node);
        Batch { node, mm, buf: Vec::new(), count: 0, capacity: 0, last_visible: -1 }
    }

    #[inline]
    pub fn begin(&mut self) {
        self.count = 0;
    }

    #[inline]
    pub fn push(&mut self, xf: &Transform3D, color: Color, custom: [f32; 4]) {
        let need = (self.count + 1) * STRIDE;
        if self.buf.len() < need {
            self.buf.resize(need.max(self.buf.len() * 2).max(STRIDE * 16), 0.0);
        }
        let b = &xf.basis;
        let o = xf.origin;
        let i = self.count * STRIDE;
        let r = &mut self.buf[i..i + STRIDE];
        let c0 = b.col_a();
        let c1 = b.col_b();
        let c2 = b.col_c();
        r[0] = c0.x;
        r[1] = c1.x;
        r[2] = c2.x;
        r[3] = o.x;
        r[4] = c0.y;
        r[5] = c1.y;
        r[6] = c2.y;
        r[7] = o.y;
        r[8] = c0.z;
        r[9] = c1.z;
        r[10] = c2.z;
        r[11] = o.z;
        r[12] = color.r;
        r[13] = color.g;
        r[14] = color.b;
        r[15] = color.a;
        r[16] = custom[0];
        r[17] = custom[1];
        r[18] = custom[2];
        r[19] = custom[3];
        self.count += 1;
    }

    pub fn finish(&mut self) {
        if self.count > self.capacity {
            let cap = (self.count * 3 / 2).max(32);
            self.capacity = cap;
            self.mm.set_instance_count(cap as i32);
            self.last_visible = -1;
        }
        if self.capacity == 0 {
            if self.last_visible != 0 {
                self.mm.set_visible_instance_count(0);
                self.last_visible = 0;
            }
            return;
        }
        let need = self.capacity * STRIDE;
        if self.buf.len() < need {
            self.buf.resize(need, 0.0);
        }
        if self.count > 0 {
            let arr = PackedFloat32Array::from(&self.buf[..need]);
            self.mm.set_buffer(&arr);
        }
        if self.last_visible != self.count as i32 {
            self.mm.set_visible_instance_count(self.count as i32);
            self.last_visible = self.count as i32;
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }
}
