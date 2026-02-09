use mlua::{FromLua, Lua, Result, UserData, UserDataFields, UserDataMethods, Value};
use nalgebra::{Isometry3, Matrix3, Rotation3, Translation3, UnitQuaternion};
use rapier3d::parry::query::intersection_test;
use rapier3d::prelude::{point, SharedShape};
use std::sync::{Arc, Mutex};

use crate::game::constants::physics as consts;
use crate::game::lua::instance::{ClassName, Instance};
use crate::game::lua::types::{CFrame, PartType, RaycastFilterType, Vector3};

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

#[inline]
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = length(v);
    if l <= 1e-8 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

#[inline]
fn mat_mul_vec(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[inline]
fn mat_t_mul_vec(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2],
    ]
}

fn intersect_box_local(
    origin: [f32; 3],
    dir: [f32; 3],
    half: [f32; 3],
) -> Option<(f32, [f32; 3])> {
    let eps = 1e-8;
    let mut t_near = f32::NEG_INFINITY;
    let mut t_far = f32::INFINITY;
    let mut near_normal = [0.0, 0.0, 0.0];
    let mut far_normal = [0.0, 0.0, 0.0];

    for axis in 0..3 {
        if dir[axis].abs() < eps {
            if origin[axis] < -half[axis] || origin[axis] > half[axis] {
                return None;
            }
            continue;
        }

        let inv = 1.0 / dir[axis];
        let mut t1 = (-half[axis] - origin[axis]) * inv;
        let mut t2 = (half[axis] - origin[axis]) * inv;

        let mut n1 = [0.0, 0.0, 0.0];
        let mut n2 = [0.0, 0.0, 0.0];
        n1[axis] = -1.0;
        n2[axis] = 1.0;

        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
            std::mem::swap(&mut n1, &mut n2);
        }

        if t1 > t_near {
            t_near = t1;
            near_normal = n1;
        }
        if t2 < t_far {
            t_far = t2;
            far_normal = n2;
        }

        if t_near > t_far {
            return None;
        }
    }

    if t_near >= 0.0 {
        Some((t_near, near_normal))
    } else if t_far >= 0.0 {
        Some((t_far, far_normal))
    } else {
        None
    }
}

fn intersect_sphere_local(origin: [f32; 3], dir: [f32; 3], radius: f32) -> Option<(f32, [f32; 3])> {
    let a = dot(dir, dir);
    let b = 2.0 * dot(origin, dir);
    let c = dot(origin, origin) - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }

    let sqrt_disc = disc.sqrt();
    let inv_2a = 1.0 / (2.0 * a);
    let t1 = (-b - sqrt_disc) * inv_2a;
    let t2 = (-b + sqrt_disc) * inv_2a;
    let t = if t1 >= 0.0 {
        t1
    } else if t2 >= 0.0 {
        t2
    } else {
        return None;
    };

    let p = [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ];
    Some((t, normalize(p)))
}

fn intersect_cylinder_local(
    origin: [f32; 3],
    dir: [f32; 3],
    radius: f32,
    half_height: f32,
) -> Option<(f32, [f32; 3])> {
    let eps = 1e-8;
    let mut best_t = f32::INFINITY;
    let mut best_normal = [0.0, 0.0, 0.0];

    let a = dir[0] * dir[0] + dir[2] * dir[2];
    let b = 2.0 * (origin[0] * dir[0] + origin[2] * dir[2]);
    let c = origin[0] * origin[0] + origin[2] * origin[2] - radius * radius;

    if a > eps {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let sqrt_disc = disc.sqrt();
            let inv_2a = 1.0 / (2.0 * a);
            for t in [(-b - sqrt_disc) * inv_2a, (-b + sqrt_disc) * inv_2a] {
                if t < 0.0 || t >= best_t {
                    continue;
                }
                let y = origin[1] + dir[1] * t;
                if y >= -half_height && y <= half_height {
                    let x = origin[0] + dir[0] * t;
                    let z = origin[2] + dir[2] * t;
                    best_t = t;
                    best_normal = normalize([x, 0.0, z]);
                }
            }
        }
    }

    if dir[1].abs() > eps {
        for y_cap in [-half_height, half_height] {
            let t = (y_cap - origin[1]) / dir[1];
            if t < 0.0 || t >= best_t {
                continue;
            }
            let x = origin[0] + dir[0] * t;
            let z = origin[2] + dir[2] * t;
            if x * x + z * z <= radius * radius {
                best_t = t;
                best_normal = if y_cap < 0.0 {
                    [0.0, -1.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
            }
        }
    }

    if best_t.is_finite() {
        Some((best_t, best_normal))
    } else {
        None
    }
}

fn intersect_wedge_local(
    origin: [f32; 3],
    dir: [f32; 3],
    half: [f32; 3],
) -> Option<(f32, [f32; 3])> {
    let eps = 1e-8;
    let hx = half[0].max(eps);
    let hy = half[1];
    let hz = half[2];

    let planes: [([f32; 3], f32); 6] = [
        ([1.0, 0.0, 0.0], hx),
        ([-1.0, 0.0, 0.0], hx),
        ([0.0, 0.0, 1.0], hz),
        ([0.0, 0.0, -1.0], hz),
        ([0.0, -1.0, 0.0], hy),
        ([hy / hx, 1.0, 0.0], 0.0), // sloped face
    ];

    let mut t_near = f32::NEG_INFINITY;
    let mut t_far = f32::INFINITY;
    let mut near_normal = [0.0, 0.0, 0.0];
    let mut far_normal = [0.0, 0.0, 0.0];

    for (normal, d) in planes {
        let denom = dot(normal, dir);
        let dist = d - dot(normal, origin);
        if denom.abs() < eps {
            if dist < 0.0 {
                return None;
            }
            continue;
        }

        let t = dist / denom;
        if denom < 0.0 {
            if t > t_near {
                t_near = t;
                near_normal = normal;
            }
        } else {
            if t < t_far {
                t_far = t;
                far_normal = normal;
            }
        }

        if t_near > t_far {
            return None;
        }
    }

    if t_near >= 0.0 {
        Some((t_near, near_normal))
    } else if t_far >= 0.0 {
        Some((t_far, far_normal))
    } else {
        None
    }
}

fn obb_intersects_obb(
    center_a: [f32; 3],
    rot_a: [[f32; 3]; 3],
    half_a: [f32; 3],
    center_b: [f32; 3],
    rot_b: [[f32; 3]; 3],
    half_b: [f32; 3],
) -> bool {
    let eps = 1e-6;
    let axes_a = [
        [rot_a[0][0], rot_a[1][0], rot_a[2][0]],
        [rot_a[0][1], rot_a[1][1], rot_a[2][1]],
        [rot_a[0][2], rot_a[1][2], rot_a[2][2]],
    ];
    let axes_b = [
        [rot_b[0][0], rot_b[1][0], rot_b[2][0]],
        [rot_b[0][1], rot_b[1][1], rot_b[2][1]],
        [rot_b[0][2], rot_b[1][2], rot_b[2][2]],
    ];

    let mut r = [[0.0f32; 3]; 3];
    let mut abs_r = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = dot(axes_a[i], axes_b[j]);
            abs_r[i][j] = r[i][j].abs() + eps;
        }
    }

    let t_world = [
        center_b[0] - center_a[0],
        center_b[1] - center_a[1],
        center_b[2] - center_a[2],
    ];
    let t = [
        dot(t_world, axes_a[0]),
        dot(t_world, axes_a[1]),
        dot(t_world, axes_a[2]),
    ];

    for i in 0..3 {
        let ra = half_a[i];
        let rb = half_b[0] * abs_r[i][0] + half_b[1] * abs_r[i][1] + half_b[2] * abs_r[i][2];
        if t[i].abs() > ra + rb {
            return false;
        }
    }

    for j in 0..3 {
        let ra = half_a[0] * abs_r[0][j] + half_a[1] * abs_r[1][j] + half_a[2] * abs_r[2][j];
        let rb = half_b[j];
        let t_proj = (t[0] * r[0][j] + t[1] * r[1][j] + t[2] * r[2][j]).abs();
        if t_proj > ra + rb {
            return false;
        }
    }

    for i in 0..3 {
        for j in 0..3 {
            let ra = half_a[(i + 1) % 3] * abs_r[(i + 2) % 3][j]
                + half_a[(i + 2) % 3] * abs_r[(i + 1) % 3][j];
            let rb = half_b[(j + 1) % 3] * abs_r[i][(j + 2) % 3]
                + half_b[(j + 2) % 3] * abs_r[i][(j + 1) % 3];
            let t_proj = (t[(i + 2) % 3] * r[(i + 1) % 3][j]
                - t[(i + 1) % 3] * r[(i + 2) % 3][j])
                .abs();
            if t_proj > ra + rb {
                return false;
            }
        }
    }

    true
}

fn sphere_intersects_obb(
    sphere_center: [f32; 3],
    sphere_radius: f32,
    obb_center: [f32; 3],
    obb_rotation: [[f32; 3]; 3],
    obb_half: [f32; 3],
) -> bool {
    let relative = [
        sphere_center[0] - obb_center[0],
        sphere_center[1] - obb_center[1],
        sphere_center[2] - obb_center[2],
    ];
    let local = mat_t_mul_vec(&obb_rotation, relative);

    let clamped = [
        local[0].max(-obb_half[0]).min(obb_half[0]),
        local[1].max(-obb_half[1]).min(obb_half[1]),
        local[2].max(-obb_half[2]).min(obb_half[2]),
    ];

    let dx = local[0] - clamped[0];
    let dy = local[1] - clamped[1];
    let dz = local[2] - clamped[2];
    (dx * dx + dy * dy + dz * dz) <= sphere_radius * sphere_radius
}

fn shape_from_part(size: Vector3, shape: PartType) -> Option<SharedShape> {
    let [sx, sy, sz] = [size.x, size.y, size.z];
    Some(match shape {
        PartType::Block => SharedShape::cuboid(sx / 2.0, sy / 2.0, sz / 2.0),
        PartType::Ball => SharedShape::ball(sx / 2.0),
        PartType::Cylinder => SharedShape::cylinder(sy / 2.0, sx / 2.0),
        PartType::Wedge => {
            let hx = sx / 2.0;
            let hy = sy / 2.0;
            let hz = sz / 2.0;
            let points = [
                point![-hx, -hy, -hz],
                point![ hx, -hy, -hz],
                point![-hx, -hy,  hz],
                point![ hx, -hy,  hz],
                point![-hx,  hy, -hz],
                point![-hx,  hy,  hz],
            ];
            SharedShape::convex_hull(&points)?
        }
    })
}

fn cframe_to_isometry(cframe: CFrame) -> Isometry3<f32> {
    let m = Matrix3::new(
        cframe.rotation[0][0], cframe.rotation[0][1], cframe.rotation[0][2],
        cframe.rotation[1][0], cframe.rotation[1][1], cframe.rotation[1][2],
        cframe.rotation[2][0], cframe.rotation[2][1], cframe.rotation[2][2],
    );
    let rot = Rotation3::from_matrix_unchecked(m);
    Isometry3::from_parts(
        Translation3::new(cframe.position.x, cframe.position.y, cframe.position.z),
        UnitQuaternion::from_rotation_matrix(&rot),
    )
}

#[derive(Clone)]
pub struct RaycastResult {
    pub instance: Instance,
    pub position: Vector3,
    pub normal: Vector3,
    pub distance: f32,
}

impl UserData for RaycastResult {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Instance", |_, this| Ok(this.instance.clone()));
        fields.add_field_method_get("Position", |_, this| Ok(this.position));
        fields.add_field_method_get("Normal", |_, this| Ok(this.normal));
        fields.add_field_method_get("Distance", |_, this| Ok(this.distance));
    }
}

#[derive(Clone)]
pub struct RaycastParams {
    pub filter_type: RaycastFilterType,
    pub filter_instances: Vec<Instance>,
    pub ignore_water: bool,
    pub collision_group: String,
    pub respect_can_collide: bool,
}

impl Default for RaycastParams {
    fn default() -> Self {
        Self {
            filter_type: RaycastFilterType::Exclude,
            filter_instances: Vec::new(),
            ignore_water: false,
            collision_group: "Default".to_string(),
            respect_can_collide: false,
        }
    }
}

impl FromLua for RaycastParams {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<RaycastParams>().map(|v| v.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "RaycastParams".to_string(),
                message: Some("expected RaycastParams".to_string()),
            }),
        }
    }
}

impl UserData for RaycastParams {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("FilterType", |_, this| Ok(this.filter_type));
        fields.add_field_method_set("FilterType", |_, this, filter_type: RaycastFilterType| {
            this.filter_type = filter_type;
            Ok(())
        });

        fields.add_field_method_get("FilterDescendantsInstances", |_, this| {
            Ok(this.filter_instances.clone())
        });
        fields.add_field_method_set(
            "FilterDescendantsInstances",
            |_, this, instances: Vec<Instance>| {
                this.filter_instances = instances;
                Ok(())
            },
        );

        fields.add_field_method_get("IgnoreWater", |_, this| Ok(this.ignore_water));
        fields.add_field_method_set("IgnoreWater", |_, this, ignore_water: bool| {
            this.ignore_water = ignore_water;
            Ok(())
        });

        fields.add_field_method_get("CollisionGroup", |_, this| Ok(this.collision_group.clone()));
        fields.add_field_method_set("CollisionGroup", |_, this, collision_group: String| {
            this.collision_group = collision_group;
            Ok(())
        });

        fields.add_field_method_get("RespectCanCollide", |_, this| Ok(this.respect_can_collide));
        fields.add_field_method_set("RespectCanCollide", |_, this, respect_can_collide: bool| {
            this.respect_can_collide = respect_can_collide;
            Ok(())
        });
    }
}

#[derive(Clone)]
pub struct OverlapParams {
    pub filter_type: RaycastFilterType,
    pub filter_instances: Vec<Instance>,
    pub max_parts: usize,
    pub collision_group: String,
    pub respect_can_collide: bool,
}

impl Default for OverlapParams {
    fn default() -> Self {
        Self {
            filter_type: RaycastFilterType::Exclude,
            filter_instances: Vec::new(),
            max_parts: 0,
            collision_group: "Default".to_string(),
            respect_can_collide: false,
        }
    }
}

impl FromLua for OverlapParams {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<OverlapParams>().map(|v| v.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "OverlapParams".to_string(),
                message: Some("expected OverlapParams".to_string()),
            }),
        }
    }
}

impl UserData for OverlapParams {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("FilterType", |_, this| Ok(this.filter_type));
        fields.add_field_method_set("FilterType", |_, this, filter_type: RaycastFilterType| {
            this.filter_type = filter_type;
            Ok(())
        });

        fields.add_field_method_get("FilterDescendantsInstances", |_, this| {
            Ok(this.filter_instances.clone())
        });
        fields.add_field_method_set(
            "FilterDescendantsInstances",
            |_, this, instances: Vec<Instance>| {
                this.filter_instances = instances;
                Ok(())
            },
        );

        fields.add_field_method_get("MaxParts", |_, this| Ok(this.max_parts as i32));
        fields.add_field_method_set("MaxParts", |_, this, max_parts: i32| {
            this.max_parts = max_parts.max(0) as usize;
            Ok(())
        });

        fields.add_field_method_get("CollisionGroup", |_, this| Ok(this.collision_group.clone()));
        fields.add_field_method_set("CollisionGroup", |_, this, collision_group: String| {
            this.collision_group = collision_group;
            Ok(())
        });

        fields.add_field_method_get("RespectCanCollide", |_, this| Ok(this.respect_can_collide));
        fields.add_field_method_set("RespectCanCollide", |_, this, respect_can_collide: bool| {
            this.respect_can_collide = respect_can_collide;
            Ok(())
        });
    }
}

pub fn register_raycast_params(lua: &Lua) -> Result<()> {
    let params_table = lua.create_table()?;

    params_table.set(
        "new",
        lua.create_function(|_, ()| Ok(RaycastParams::default()))?,
    )?;

    lua.globals().set("RaycastParams", params_table)?;

    Ok(())
}

pub fn register_overlap_params(lua: &Lua) -> Result<()> {
    let params_table = lua.create_table()?;

    params_table.set(
        "new",
        lua.create_function(|_, ()| Ok(OverlapParams::default()))?,
    )?;

    lua.globals().set("OverlapParams", params_table)?;

    Ok(())
}

pub struct WorkspaceServiceData {
    pub gravity: f32,
    pub current_camera: Option<Instance>,
    pub children: Vec<Instance>,
}

impl WorkspaceServiceData {
    pub fn new() -> Self {
        Self {
            gravity: consts::DEFAULT_GRAVITY,
            current_camera: None,
            children: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct WorkspaceService {
    pub instance: Instance,
    pub data: Arc<Mutex<WorkspaceServiceData>>,
}

impl WorkspaceService {
    pub fn new() -> Self {
        let instance = Instance::new(ClassName::Workspace, "Workspace");
        Self {
            instance,
            data: Arc::new(Mutex::new(WorkspaceServiceData::new())),
        }
    }

    pub fn add_child(&self, child: Instance) {
        child.set_parent(Some(&self.instance));
        self.data.lock().unwrap().children.push(child);
    }

    pub fn get_children(&self) -> Vec<Instance> {
        self.instance.get_children()
    }

    pub fn get_descendants(&self) -> Vec<Instance> {
        self.instance.get_descendants()
    }

    pub fn raycast(
        &self,
        origin: Vector3,
        direction: Vector3,
        params: Option<RaycastParams>,
    ) -> Option<RaycastResult> {
        let params = params.unwrap_or_default();
        let ray_length = direction.magnitude();
        let ray_dir = direction.unit();

        let mut closest: Option<(f32, Instance, Vector3)> = None;

        for descendant in self.get_descendants() {
            // Extract part data while holding the lock
            let part_info = {
                let data = descendant.data.lock().unwrap();
                data.part_data.as_ref().map(|part| {
                    (
                        part.can_query,
                        part.can_collide,
                        part.collision_group.clone(),
                        part.size,
                        part.cframe.position,
                        part.cframe.rotation,
                        part.shape,
                    )
                })
            }; // Lock released here

            let Some((can_query, can_collide, collision_group, size, position, rotation, shape)) = part_info else {
                continue;
            };

            if !can_query {
                continue;
            }

            if params.respect_can_collide && !can_collide {
                continue;
            }
            if !params.collision_group.is_empty()
                && params.collision_group != "Default"
                && collision_group != params.collision_group
            {
                continue;
            }

            // Now check filtering without holding any lock
            let should_filter = params.filter_instances.iter().any(|i| {
                i.id() == descendant.id() || descendant.is_descendant_of(i)
            });

            let skip = match params.filter_type {
                RaycastFilterType::Exclude => should_filter,
                RaycastFilterType::Include => !should_filter,
            };

            if skip {
                continue;
            }

            let center = [position.x, position.y, position.z];
            let half = [size.x / 2.0, size.y / 2.0, size.z / 2.0];
            let ray_origin = [origin.x, origin.y, origin.z];
            let ray_direction = [ray_dir.x, ray_dir.y, ray_dir.z];

            let local_origin = mat_t_mul_vec(
                &rotation,
                [
                    ray_origin[0] - center[0],
                    ray_origin[1] - center[1],
                    ray_origin[2] - center[2],
                ],
            );
            let local_dir = mat_t_mul_vec(&rotation, ray_direction);

            let hit = match shape {
                PartType::Block => intersect_box_local(local_origin, local_dir, half),
                PartType::Ball => intersect_sphere_local(local_origin, local_dir, half[0]),
                PartType::Cylinder => intersect_cylinder_local(local_origin, local_dir, half[0], half[1]),
                PartType::Wedge => intersect_wedge_local(local_origin, local_dir, half),
            };

            if let Some((t, local_normal)) = hit {
                if t < 0.0 || t > ray_length {
                    continue;
                }
                let world_normal = normalize(mat_mul_vec(&rotation, local_normal));
                let normal = Vector3::new(world_normal[0], world_normal[1], world_normal[2]);

                if closest.is_none() || t < closest.as_ref().unwrap().0 {
                    closest = Some((t, descendant.clone(), normal));
                }
            }
        }

        closest.map(|(dist, instance, normal)| {
            let position = Vector3::new(
                origin.x + ray_dir.x * dist,
                origin.y + ray_dir.y * dist,
                origin.z + ray_dir.z * dist,
            );
            RaycastResult {
                instance,
                position,
                normal,
                distance: dist,
            }
        })
    }

    pub fn get_part_bounds_in_box(
        &self,
        cframe: CFrame,
        size: Vector3,
        params: Option<OverlapParams>,
    ) -> Vec<Instance> {
        let params = params.unwrap_or_default();
        let query_center = [cframe.position.x, cframe.position.y, cframe.position.z];
        let query_rot = cframe.rotation;
        let query_half = [size.x / 2.0, size.y / 2.0, size.z / 2.0];

        let mut out = Vec::new();
        for inst in self.get_descendants() {
            let should_filter = params.filter_instances.iter().any(|i| {
                i.id() == inst.id() || inst.is_descendant_of(i)
            });
            let skip = match params.filter_type {
                RaycastFilterType::Exclude => should_filter,
                RaycastFilterType::Include => !should_filter,
            };
            if skip {
                continue;
            }

            let intersects = {
                let data = inst.data.lock().unwrap();
                if let Some(part) = &data.part_data {
                    if !part.can_query {
                        false
                    } else if params.respect_can_collide && !part.can_collide {
                        false
                    } else if !params.collision_group.is_empty()
                        && params.collision_group != "Default"
                        && part.collision_group != params.collision_group
                    {
                        false
                    } else {
                        let part_center = [part.position.x, part.position.y, part.position.z];
                        let part_rot = part.cframe.rotation;
                        let part_half = [part.size.x / 2.0, part.size.y / 2.0, part.size.z / 2.0];

                        obb_intersects_obb(
                            query_center,
                            query_rot,
                            query_half,
                            part_center,
                            part_rot,
                            part_half,
                        )
                    }
                } else {
                    false
                }
            };

            if intersects {
                out.push(inst);
                if params.max_parts > 0 && out.len() >= params.max_parts {
                    break;
                }
            }
        }
        out
    }

    pub fn get_part_bounds_in_radius(
        &self,
        position: Vector3,
        radius: f32,
        params: Option<OverlapParams>,
    ) -> Vec<Instance> {
        let params = params.unwrap_or_default();
        let sphere_center = [position.x, position.y, position.z];
        let mut out = Vec::new();
        for inst in self.get_descendants() {
            let should_filter = params.filter_instances.iter().any(|i| {
                i.id() == inst.id() || inst.is_descendant_of(i)
            });
            let skip = match params.filter_type {
                RaycastFilterType::Exclude => should_filter,
                RaycastFilterType::Include => !should_filter,
            };
            if skip {
                continue;
            }

            let intersects = {
                let data = inst.data.lock().unwrap();
                if let Some(part) = &data.part_data {
                    if !part.can_query {
                        false
                    } else if params.respect_can_collide && !part.can_collide {
                        false
                    } else if !params.collision_group.is_empty()
                        && params.collision_group != "Default"
                        && part.collision_group != params.collision_group
                    {
                        false
                    } else {
                        let part_center = [part.position.x, part.position.y, part.position.z];
                        let part_rot = part.cframe.rotation;
                        let part_half = [part.size.x / 2.0, part.size.y / 2.0, part.size.z / 2.0];
                        sphere_intersects_obb(sphere_center, radius, part_center, part_rot, part_half)
                    }
                } else {
                    false
                }
            };

            if intersects {
                out.push(inst);
                if params.max_parts > 0 && out.len() >= params.max_parts {
                    break;
                }
            }
        }
        out
    }

    pub fn get_parts_in_part(
        &self,
        query_part: Instance,
        params: Option<OverlapParams>,
    ) -> Vec<Instance> {
        let params = params.unwrap_or_default();
        let query_data = {
            let data = query_part.data.lock().unwrap();
            let Some(part) = &data.part_data else {
                return Vec::new();
            };
            (data.id.0, part.size, part.cframe, part.shape)
        };
        let (query_id, query_size, query_cframe, query_shape_type) = query_data;
        let Some(query_shape) = shape_from_part(query_size, query_shape_type) else {
            return Vec::new();
        };
        let query_iso = cframe_to_isometry(query_cframe);

        let mut out = Vec::new();
        for inst in self.get_descendants() {
            let should_filter = params.filter_instances.iter().any(|i| {
                i.id() == inst.id() || inst.is_descendant_of(i)
            });
            let skip = match params.filter_type {
                RaycastFilterType::Exclude => should_filter,
                RaycastFilterType::Include => !should_filter,
            };
            if skip {
                continue;
            }

            let part_data = {
                let data = inst.data.lock().unwrap();
                if data.id.0 == query_id {
                    None
                } else {
                    data.part_data.as_ref().map(|p| {
                        (
                            p.can_query,
                            p.can_collide,
                            p.collision_group.clone(),
                            p.size,
                            p.cframe,
                            p.shape,
                        )
                    })
                }
            };

            let Some((can_query, can_collide, collision_group, size, cframe, shape_type)) = part_data else {
                continue;
            };

            if !can_query {
                continue;
            }
            if params.respect_can_collide && !can_collide {
                continue;
            }
            if !params.collision_group.is_empty()
                && params.collision_group != "Default"
                && collision_group != params.collision_group
            {
                continue;
            }

            let Some(shape) = shape_from_part(size, shape_type) else {
                continue;
            };
            let iso = cframe_to_isometry(cframe);
            let intersects = intersection_test(&query_iso, query_shape.as_ref(), &iso, shape.as_ref())
                .unwrap_or(false);
            if intersects {
                out.push(inst);
                if params.max_parts > 0 && out.len() >= params.max_parts {
                    break;
                }
            }
        }
        out
    }
}

impl UserData for WorkspaceService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Gravity", |_, this| Ok(this.data.lock().unwrap().gravity));
        fields.add_field_method_set("Gravity", |_, this, gravity: f32| {
            this.data.lock().unwrap().gravity = gravity;
            Ok(())
        });

        fields.add_field_method_get("CurrentCamera", |_, this| {
            Ok(this.data.lock().unwrap().current_camera.clone())
        });

        fields.add_field_method_get("Name", |_, _| Ok("Workspace".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("Workspace".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "Raycast",
            |_, this, (origin, direction, params): (Vector3, Vector3, Option<RaycastParams>)| {
                Ok(this.raycast(origin, direction, params))
            },
        );

        methods.add_method(
            "GetPartBoundsInBox",
            |_, this, (cframe, size, params): (CFrame, Vector3, Option<OverlapParams>)| {
                Ok(this.get_part_bounds_in_box(cframe, size, params))
            },
        );

        methods.add_method(
            "GetPartBoundsInRadius",
            |_, this, (position, radius, params): (Vector3, f32, Option<OverlapParams>)| {
                Ok(this.get_part_bounds_in_radius(position, radius, params))
            },
        );

        methods.add_method(
            "GetPartsInPart",
            |_, this, (part, params): (Instance, Option<OverlapParams>)| {
                Ok(this.get_parts_in_part(part, params))
            },
        );

        methods.add_method("GetChildren", |_, this, ()| Ok(this.get_children()));

        methods.add_method("GetDescendants", |_, this, ()| Ok(this.get_descendants()));

        methods.add_method(
            "FindFirstChild",
            |_, this, (name, recursive): (String, Option<bool>)| {
                Ok(this
                    .instance
                    .find_first_child(&name, recursive.unwrap_or(false)))
            },
        );

        methods.add_method("FindFirstChildOfClass", |_, this, class_name: String| {
            Ok(this.instance.find_first_child_of_class(&class_name))
        });
    }
}
