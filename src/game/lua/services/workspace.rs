use mlua::{FromLua, Lua, Result, UserData, UserDataFields, UserDataMethods, Value};
use std::sync::{Arc, Mutex};

use crate::game::constants::physics as consts;
use crate::game::lua::instance::{ClassName, Instance};
use crate::game::lua::types::{CFrame, RaycastFilterType, Vector3};

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
}

impl Default for RaycastParams {
    fn default() -> Self {
        Self {
            filter_type: RaycastFilterType::Exclude,
            filter_instances: Vec::new(),
            ignore_water: false,
            collision_group: "Default".to_string(),
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
                    (part.can_collide, part.size, part.position)
                })
            }; // Lock released here

            let Some((can_collide, size, position)) = part_info else {
                continue;
            };

            if !can_collide {
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

            let half_size = Vector3::new(
                size.x / 2.0,
                size.y / 2.0,
                size.z / 2.0,
            );

            let center = position;

            // Slab intersection with proper handling for zero direction components
            // When ray is parallel to a slab (dir component = 0):
            // - If origin is outside the slab, no intersection (t_near > t_far)
            // - If origin is inside the slab, the slab doesn't constrain t (use -inf to +inf)
            let (t1_x, t2_x) = if ray_dir.x.abs() < 1e-10 {
                // Ray parallel to YZ plane
                if origin.x < center.x - half_size.x || origin.x > center.x + half_size.x {
                    (f32::INFINITY, f32::NEG_INFINITY) // No intersection
                } else {
                    (f32::NEG_INFINITY, f32::INFINITY) // Always inside this slab
                }
            } else {
                let t_min = (center.x - half_size.x - origin.x) / ray_dir.x;
                let t_max = (center.x + half_size.x - origin.x) / ray_dir.x;
                (t_min.min(t_max), t_min.max(t_max))
            };

            let (t1_y, t2_y) = if ray_dir.y.abs() < 1e-10 {
                // Ray parallel to XZ plane
                if origin.y < center.y - half_size.y || origin.y > center.y + half_size.y {
                    (f32::INFINITY, f32::NEG_INFINITY) // No intersection
                } else {
                    (f32::NEG_INFINITY, f32::INFINITY) // Always inside this slab
                }
            } else {
                let t_min = (center.y - half_size.y - origin.y) / ray_dir.y;
                let t_max = (center.y + half_size.y - origin.y) / ray_dir.y;
                (t_min.min(t_max), t_min.max(t_max))
            };

            let (t1_z, t2_z) = if ray_dir.z.abs() < 1e-10 {
                // Ray parallel to XY plane
                if origin.z < center.z - half_size.z || origin.z > center.z + half_size.z {
                    (f32::INFINITY, f32::NEG_INFINITY) // No intersection
                } else {
                    (f32::NEG_INFINITY, f32::INFINITY) // Always inside this slab
                }
            } else {
                let t_min = (center.z - half_size.z - origin.z) / ray_dir.z;
                let t_max = (center.z + half_size.z - origin.z) / ray_dir.z;
                (t_min.min(t_max), t_min.max(t_max))
            };

            let t_near = t1_x.max(t1_y).max(t1_z);
            let t_far = t2_x.min(t2_y).min(t2_z);

            if t_near <= t_far && t_near >= 0.0 && t_near <= ray_length {
                let hit_pos = Vector3::new(
                    origin.x + ray_dir.x * t_near,
                    origin.y + ray_dir.y * t_near,
                    origin.z + ray_dir.z * t_near,
                );

                let epsilon = 0.001;
                let normal = if (hit_pos.x - (center.x - half_size.x)).abs() < epsilon {
                    Vector3::new(-1.0, 0.0, 0.0)
                } else if (hit_pos.x - (center.x + half_size.x)).abs() < epsilon {
                    Vector3::new(1.0, 0.0, 0.0)
                } else if (hit_pos.y - (center.y - half_size.y)).abs() < epsilon {
                    Vector3::new(0.0, -1.0, 0.0)
                } else if (hit_pos.y - (center.y + half_size.y)).abs() < epsilon {
                    Vector3::new(0.0, 1.0, 0.0)
                } else if (hit_pos.z - (center.z - half_size.z)).abs() < epsilon {
                    Vector3::new(0.0, 0.0, -1.0)
                } else {
                    Vector3::new(0.0, 0.0, 1.0)
                };

                if closest.is_none() || t_near < closest.as_ref().unwrap().0 {
                    closest = Some((t_near, descendant.clone(), normal));
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

    pub fn get_part_bounds_in_box(&self, cframe: CFrame, size: Vector3) -> Vec<Instance> {
        let center = cframe.position;
        let half = Vector3::new(size.x / 2.0, size.y / 2.0, size.z / 2.0);

        self.get_descendants()
            .into_iter()
            .filter(|inst| {
                let data = inst.data.lock().unwrap();
                if let Some(part) = &data.part_data {
                    let pos = part.position;
                    pos.x >= center.x - half.x
                        && pos.x <= center.x + half.x
                        && pos.y >= center.y - half.y
                        && pos.y <= center.y + half.y
                        && pos.z >= center.z - half.z
                        && pos.z <= center.z + half.z
                } else {
                    false
                }
            })
            .collect()
    }

    pub fn get_part_bounds_in_radius(&self, position: Vector3, radius: f32) -> Vec<Instance> {
        let radius_sq = radius * radius;
        self.get_descendants()
            .into_iter()
            .filter(|inst| {
                let data = inst.data.lock().unwrap();
                if let Some(part) = &data.part_data {
                    let dx = part.position.x - position.x;
                    let dy = part.position.y - position.y;
                    let dz = part.position.z - position.z;
                    dx * dx + dy * dy + dz * dz <= radius_sq
                } else {
                    false
                }
            })
            .collect()
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
            |_, this, (cframe, size): (CFrame, Vector3)| {
                Ok(this.get_part_bounds_in_box(cframe, size))
            },
        );

        methods.add_method(
            "GetPartBoundsInRadius",
            |_, this, (position, radius): (Vector3, f32)| {
                Ok(this.get_part_bounds_in_radius(position, radius))
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
