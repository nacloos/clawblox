use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

use super::vector3::Vector3;

#[derive(Debug, Clone, Copy)]
pub struct CFrame {
    pub position: Vector3,
    pub rotation: [[f32; 3]; 3],
}

impl Default for CFrame {
    fn default() -> Self {
        Self::identity()
    }
}

impl CFrame {
    pub fn identity() -> Self {
        Self {
            position: Vector3::zero(),
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            position: Vector3::new(x, y, z),
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn from_position(pos: Vector3) -> Self {
        Self {
            position: pos,
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn look_at(pos: Vector3, target: Vector3, up: Option<Vector3>) -> Self {
        let up = up.unwrap_or(Vector3::new(0.0, 1.0, 0.0));

        let look = (target.x - pos.x, target.y - pos.y, target.z - pos.z);
        let look_len = (look.0 * look.0 + look.1 * look.1 + look.2 * look.2).sqrt();

        if look_len < 1e-6 {
            return Self::from_position(pos);
        }

        let look = (look.0 / look_len, look.1 / look_len, look.2 / look_len);

        // right = look × up (standard lookAt convention)
        let right = (
            look.1 * up.z - look.2 * up.y,
            look.2 * up.x - look.0 * up.z,
            look.0 * up.y - look.1 * up.x,
        );
        let right_len = (right.0 * right.0 + right.1 * right.1 + right.2 * right.2).sqrt();

        if right_len < 1e-6 {
            return Self::from_position(pos);
        }

        let right = (
            right.0 / right_len,
            right.1 / right_len,
            right.2 / right_len,
        );

        // up_vec = right × look (to ensure orthonormal basis)
        let up_vec = (
            right.1 * look.2 - right.2 * look.1,
            right.2 * look.0 - right.0 * look.2,
            right.0 * look.1 - right.1 * look.0,
        );

        Self {
            position: pos,
            rotation: [
                [right.0, up_vec.0, -look.0],
                [right.1, up_vec.1, -look.1],
                [right.2, up_vec.2, -look.2],
            ],
        }
    }

    pub fn from_euler_angles_xyz(rx: f32, ry: f32, rz: f32) -> Self {
        let (sx, cx) = rx.sin_cos();
        let (sy, cy) = ry.sin_cos();
        let (sz, cz) = rz.sin_cos();

        Self {
            position: Vector3::zero(),
            rotation: [
                [cy * cz, -cy * sz, sy],
                [sx * sy * cz + cx * sz, -sx * sy * sz + cx * cz, -sx * cy],
                [-cx * sy * cz + sx * sz, cx * sy * sz + sx * cz, cx * cy],
            ],
        }
    }

    pub fn look_vector(&self) -> Vector3 {
        Vector3::new(-self.rotation[0][2], -self.rotation[1][2], -self.rotation[2][2])
    }

    pub fn right_vector(&self) -> Vector3 {
        Vector3::new(self.rotation[0][0], self.rotation[1][0], self.rotation[2][0])
    }

    pub fn up_vector(&self) -> Vector3 {
        Vector3::new(self.rotation[0][1], self.rotation[1][1], self.rotation[2][1])
    }

    pub fn inverse(&self) -> Self {
        let inv_rot = [
            [
                self.rotation[0][0],
                self.rotation[1][0],
                self.rotation[2][0],
            ],
            [
                self.rotation[0][1],
                self.rotation[1][1],
                self.rotation[2][1],
            ],
            [
                self.rotation[0][2],
                self.rotation[1][2],
                self.rotation[2][2],
            ],
        ];

        let inv_pos = Vector3::new(
            -(inv_rot[0][0] * self.position.x
                + inv_rot[0][1] * self.position.y
                + inv_rot[0][2] * self.position.z),
            -(inv_rot[1][0] * self.position.x
                + inv_rot[1][1] * self.position.y
                + inv_rot[1][2] * self.position.z),
            -(inv_rot[2][0] * self.position.x
                + inv_rot[2][1] * self.position.y
                + inv_rot[2][2] * self.position.z),
        );

        Self {
            position: inv_pos,
            rotation: inv_rot,
        }
    }

    pub fn lerp(&self, goal: &CFrame, alpha: f32) -> Self {
        Self {
            position: self.position.lerp(&goal.position, alpha),
            rotation: [
                [
                    self.rotation[0][0] + (goal.rotation[0][0] - self.rotation[0][0]) * alpha,
                    self.rotation[0][1] + (goal.rotation[0][1] - self.rotation[0][1]) * alpha,
                    self.rotation[0][2] + (goal.rotation[0][2] - self.rotation[0][2]) * alpha,
                ],
                [
                    self.rotation[1][0] + (goal.rotation[1][0] - self.rotation[1][0]) * alpha,
                    self.rotation[1][1] + (goal.rotation[1][1] - self.rotation[1][1]) * alpha,
                    self.rotation[1][2] + (goal.rotation[1][2] - self.rotation[1][2]) * alpha,
                ],
                [
                    self.rotation[2][0] + (goal.rotation[2][0] - self.rotation[2][0]) * alpha,
                    self.rotation[2][1] + (goal.rotation[2][1] - self.rotation[2][1]) * alpha,
                    self.rotation[2][2] + (goal.rotation[2][2] - self.rotation[2][2]) * alpha,
                ],
            ],
        }
    }

    pub fn multiply(&self, other: &CFrame) -> Self {
        let new_pos = Vector3::new(
            self.rotation[0][0] * other.position.x
                + self.rotation[0][1] * other.position.y
                + self.rotation[0][2] * other.position.z
                + self.position.x,
            self.rotation[1][0] * other.position.x
                + self.rotation[1][1] * other.position.y
                + self.rotation[1][2] * other.position.z
                + self.position.y,
            self.rotation[2][0] * other.position.x
                + self.rotation[2][1] * other.position.y
                + self.rotation[2][2] * other.position.z
                + self.position.z,
        );

        let mut new_rot = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                for k in 0..3 {
                    new_rot[i][j] += self.rotation[i][k] * other.rotation[k][j];
                }
            }
        }

        Self {
            position: new_pos,
            rotation: new_rot,
        }
    }

    pub fn point_to_world_space(&self, point: &Vector3) -> Vector3 {
        Vector3::new(
            self.rotation[0][0] * point.x
                + self.rotation[0][1] * point.y
                + self.rotation[0][2] * point.z
                + self.position.x,
            self.rotation[1][0] * point.x
                + self.rotation[1][1] * point.y
                + self.rotation[1][2] * point.z
                + self.position.y,
            self.rotation[2][0] * point.x
                + self.rotation[2][1] * point.y
                + self.rotation[2][2] * point.z
                + self.position.z,
        )
    }

    pub fn point_to_object_space(&self, point: &Vector3) -> Vector3 {
        let rel = Vector3::new(
            point.x - self.position.x,
            point.y - self.position.y,
            point.z - self.position.z,
        );
        Vector3::new(
            self.rotation[0][0] * rel.x
                + self.rotation[1][0] * rel.y
                + self.rotation[2][0] * rel.z,
            self.rotation[0][1] * rel.x
                + self.rotation[1][1] * rel.y
                + self.rotation[2][1] * rel.z,
            self.rotation[0][2] * rel.x
                + self.rotation[1][2] * rel.y
                + self.rotation[2][2] * rel.z,
        )
    }

    pub fn to_world_space(&self, cf: &CFrame) -> CFrame {
        self.multiply(cf)
    }

    pub fn to_object_space(&self, cf: &CFrame) -> CFrame {
        self.inverse().multiply(cf)
    }

    pub fn get_components(&self) -> [f32; 12] {
        [
            self.position.x,
            self.position.y,
            self.position.z,
            self.rotation[0][0],
            self.rotation[0][1],
            self.rotation[0][2],
            self.rotation[1][0],
            self.rotation[1][1],
            self.rotation[1][2],
            self.rotation[2][0],
            self.rotation[2][1],
            self.rotation[2][2],
        ]
    }
}

impl FromLua for CFrame {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<CFrame>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "CFrame".to_string(),
                message: Some("expected CFrame".to_string()),
            }),
        }
    }
}

impl UserData for CFrame {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Position", |_, this| Ok(this.position));
        fields.add_field_method_get("LookVector", |_, this| Ok(this.look_vector()));
        fields.add_field_method_get("RightVector", |_, this| Ok(this.right_vector()));
        fields.add_field_method_get("UpVector", |_, this| Ok(this.up_vector()));
        fields.add_field_method_get("X", |_, this| Ok(this.position.x));
        fields.add_field_method_get("Y", |_, this| Ok(this.position.y));
        fields.add_field_method_get("Z", |_, this| Ok(this.position.z));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Inverse", |_, this, ()| Ok(this.inverse()));
        methods.add_method("Lerp", |_, this, (goal, alpha): (CFrame, f32)| {
            Ok(this.lerp(&goal, alpha))
        });
        methods.add_method("ToWorldSpace", |_, this, cf: CFrame| {
            Ok(this.to_world_space(&cf))
        });
        methods.add_method("ToObjectSpace", |_, this, cf: CFrame| {
            Ok(this.to_object_space(&cf))
        });
        methods.add_method("PointToWorldSpace", |_, this, v: Vector3| {
            Ok(this.point_to_world_space(&v))
        });
        methods.add_method("PointToObjectSpace", |_, this, v: Vector3| {
            Ok(this.point_to_object_space(&v))
        });
        methods.add_method("GetComponents", |_, this, ()| {
            let c = this.get_components();
            Ok((c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7], c[8], c[9], c[10], c[11]))
        });

        methods.add_meta_method(MetaMethod::Mul, |lua, this, value: mlua::Value| {
            match value {
                mlua::Value::UserData(ud) => {
                    if let Ok(cf) = ud.borrow::<CFrame>() {
                        Ok(mlua::Value::UserData(
                            lua.create_userdata(this.multiply(&cf))?,
                        ))
                    } else if let Ok(v) = ud.borrow::<Vector3>() {
                        Ok(mlua::Value::UserData(
                            lua.create_userdata(this.point_to_world_space(&v))?,
                        ))
                    } else {
                        Err(mlua::Error::runtime("Expected CFrame or Vector3"))
                    }
                }
                _ => Err(mlua::Error::runtime("Expected CFrame or Vector3")),
            }
        });

        methods.add_meta_method(MetaMethod::Add, |_, this, v: Vector3| {
            Ok(CFrame {
                position: Vector3::new(
                    this.position.x + v.x,
                    this.position.y + v.y,
                    this.position.z + v.z,
                ),
                rotation: this.rotation,
            })
        });

        methods.add_meta_method(MetaMethod::Sub, |_, this, v: Vector3| {
            Ok(CFrame {
                position: Vector3::new(
                    this.position.x - v.x,
                    this.position.y - v.y,
                    this.position.z - v.z,
                ),
                rotation: this.rotation,
            })
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "CFrame({}, {}, {})",
                this.position.x, this.position.y, this.position.z
            ))
        });
    }
}

pub fn register_cframe(lua: &Lua) -> Result<()> {
    let cframe_table = lua.create_table()?;

    cframe_table.set(
        "new",
        lua.create_function(
            |inner_lua, args: mlua::MultiValue| -> Result<CFrame> {
                match args.len() {
                    0 => Ok(CFrame::identity()),
                    3 => {
                        let mut iter = args.into_iter();
                        let x: f32 = mlua::FromLua::from_lua(iter.next().unwrap(), inner_lua)?;
                        let y: f32 = mlua::FromLua::from_lua(iter.next().unwrap(), inner_lua)?;
                        let z: f32 = mlua::FromLua::from_lua(iter.next().unwrap(), inner_lua)?;
                        Ok(CFrame::new(x, y, z))
                    }
                    2 => {
                        let mut iter = args.into_iter();
                        let pos: Vector3 = mlua::FromLua::from_lua(iter.next().unwrap(), inner_lua)?;
                        let target: Vector3 = mlua::FromLua::from_lua(iter.next().unwrap(), inner_lua)?;
                        Ok(CFrame::look_at(pos, target, None))
                    }
                    _ => Err(mlua::Error::runtime("Invalid arguments to CFrame.new")),
                }
            },
        )?,
    )?;

    cframe_table.set(
        "lookAt",
        lua.create_function(
            |_, (pos, target, up): (Vector3, Vector3, Option<Vector3>)| {
                Ok(CFrame::look_at(pos, target, up))
            },
        )?,
    )?;

    cframe_table.set(
        "fromEulerAnglesXYZ",
        lua.create_function(|_, (rx, ry, rz): (f32, f32, f32)| {
            Ok(CFrame::from_euler_angles_xyz(rx, ry, rz))
        })?,
    )?;

    cframe_table.set(
        "Angles",
        lua.create_function(|_, (rx, ry, rz): (f32, f32, f32)| {
            Ok(CFrame::from_euler_angles_xyz(rx, ry, rz))
        })?,
    )?;

    lua.globals().set("CFrame", cframe_table)?;

    Ok(())
}
