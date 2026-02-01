use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

#[derive(Debug, Clone, Copy, Default)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    pub fn one() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }

    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn unit(&self) -> Self {
        let mag = self.magnitude();
        if mag > 0.0 {
            Self::new(self.x / mag, self.y / mag, self.z / mag)
        } else {
            Self::zero()
        }
    }

    pub fn dot(&self, other: &Vector3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(&self, other: &Vector3) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn lerp(&self, goal: &Vector3, alpha: f32) -> Self {
        Self::new(
            self.x + (goal.x - self.x) * alpha,
            self.y + (goal.y - self.y) * alpha,
            self.z + (goal.z - self.z) * alpha,
        )
    }

    pub fn fuzzy_eq(&self, other: &Vector3, epsilon: f32) -> bool {
        (self.x - other.x).abs() < epsilon
            && (self.y - other.y).abs() < epsilon
            && (self.z - other.z).abs() < epsilon
    }

    pub fn to_array(&self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn from_array(arr: [f32; 3]) -> Self {
        Self::new(arr[0], arr[1], arr[2])
    }
}

impl FromLua for Vector3 {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Vector3>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Vector3".to_string(),
                message: Some("expected Vector3".to_string()),
            }),
        }
    }
}

impl UserData for Vector3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |_, this| Ok(this.x));
        fields.add_field_method_get("Y", |_, this| Ok(this.y));
        fields.add_field_method_get("Z", |_, this| Ok(this.z));
        fields.add_field_method_get("Magnitude", |_, this| Ok(this.magnitude()));
        fields.add_field_method_get("Unit", |_, this| Ok(this.unit()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Dot", |_, this, other: Vector3| Ok(this.dot(&other)));
        methods.add_method("Cross", |_, this, other: Vector3| Ok(this.cross(&other)));
        methods.add_method("Lerp", |_, this, (goal, alpha): (Vector3, f32)| {
            Ok(this.lerp(&goal, alpha))
        });
        methods.add_method(
            "FuzzyEq",
            |_, this, (other, epsilon): (Vector3, Option<f32>)| {
                Ok(this.fuzzy_eq(&other, epsilon.unwrap_or(1e-5)))
            },
        );
        // Also expose Unit as a method for scripts that use v:Unit() syntax
        methods.add_method("Unit", |_, this, ()| Ok(this.unit()));

        methods.add_meta_method(MetaMethod::Add, |_, this, other: Vector3| {
            Ok(Vector3::new(
                this.x + other.x,
                this.y + other.y,
                this.z + other.z,
            ))
        });

        methods.add_meta_method(MetaMethod::Sub, |_, this, other: Vector3| {
            Ok(Vector3::new(
                this.x - other.x,
                this.y - other.y,
                this.z - other.z,
            ))
        });

        methods.add_meta_method(MetaMethod::Mul, |_, this, value: mlua::Value| {
            match value {
                mlua::Value::Number(n) => {
                    let n = n as f32;
                    Ok(Vector3::new(this.x * n, this.y * n, this.z * n))
                }
                mlua::Value::Integer(n) => {
                    let n = n as f32;
                    Ok(Vector3::new(this.x * n, this.y * n, this.z * n))
                }
                mlua::Value::UserData(ud) => {
                    let other = ud.borrow::<Vector3>()?;
                    Ok(Vector3::new(
                        this.x * other.x,
                        this.y * other.y,
                        this.z * other.z,
                    ))
                }
                _ => Err(mlua::Error::runtime("Expected number or Vector3")),
            }
        });

        methods.add_meta_method(MetaMethod::Div, |_, this, value: mlua::Value| {
            match value {
                mlua::Value::Number(n) => {
                    let n = n as f32;
                    Ok(Vector3::new(this.x / n, this.y / n, this.z / n))
                }
                mlua::Value::Integer(n) => {
                    let n = n as f32;
                    Ok(Vector3::new(this.x / n, this.y / n, this.z / n))
                }
                mlua::Value::UserData(ud) => {
                    let other = ud.borrow::<Vector3>()?;
                    Ok(Vector3::new(
                        this.x / other.x,
                        this.y / other.y,
                        this.z / other.z,
                    ))
                }
                _ => Err(mlua::Error::runtime("Expected number or Vector3")),
            }
        });

        methods.add_meta_method(MetaMethod::Unm, |_, this, ()| {
            Ok(Vector3::new(-this.x, -this.y, -this.z))
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: Vector3| {
            Ok(this.x == other.x && this.y == other.y && this.z == other.z)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Vector3({}, {}, {})", this.x, this.y, this.z))
        });
    }
}

pub fn register_vector3(lua: &Lua) -> Result<()> {
    let vector3_table = lua.create_table()?;

    vector3_table.set(
        "new",
        lua.create_function(|_, (x, y, z): (Option<f32>, Option<f32>, Option<f32>)| {
            Ok(Vector3::new(
                x.unwrap_or(0.0),
                y.unwrap_or(0.0),
                z.unwrap_or(0.0),
            ))
        })?,
    )?;

    vector3_table.set("zero", Vector3::zero())?;
    vector3_table.set("one", Vector3::one())?;
    vector3_table.set("xAxis", Vector3::new(1.0, 0.0, 0.0))?;
    vector3_table.set("yAxis", Vector3::new(0.0, 1.0, 0.0))?;
    vector3_table.set("zAxis", Vector3::new(0.0, 0.0, 1.0))?;

    lua.globals().set("Vector3", vector3_table)?;

    Ok(())
}
