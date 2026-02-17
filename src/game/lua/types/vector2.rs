use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

#[derive(Debug, Clone, Copy, Default)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0)
    }

    pub fn one() -> Self {
        Self::new(1.0, 1.0)
    }

    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn unit(&self) -> Self {
        let mag = self.magnitude();
        if mag > 0.0 {
            Self::new(self.x / mag, self.y / mag)
        } else {
            Self::zero()
        }
    }

    pub fn dot(&self, other: &Vector2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn lerp(&self, goal: &Vector2, alpha: f32) -> Self {
        Self::new(
            self.x + (goal.x - self.x) * alpha,
            self.y + (goal.y - self.y) * alpha,
        )
    }
}

impl FromLua for Vector2 {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Vector2>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Vector2".to_string(),
                message: Some("expected Vector2".to_string()),
            }),
        }
    }
}

impl UserData for Vector2 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |_, this| Ok(this.x));
        fields.add_field_method_get("Y", |_, this| Ok(this.y));
        fields.add_field_method_get("Magnitude", |_, this| Ok(this.magnitude()));
        fields.add_field_method_get("Unit", |_, this| Ok(this.unit()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Dot", |_, this, other: Vector2| Ok(this.dot(&other)));
        methods.add_method("Lerp", |_, this, (goal, alpha): (Vector2, f32)| {
            Ok(this.lerp(&goal, alpha))
        });
        methods.add_method("Unit", |_, this, ()| Ok(this.unit()));

        methods.add_meta_method(MetaMethod::Add, |_, this, other: Vector2| {
            Ok(Vector2::new(this.x + other.x, this.y + other.y))
        });
        methods.add_meta_method(MetaMethod::Sub, |_, this, other: Vector2| {
            Ok(Vector2::new(this.x - other.x, this.y - other.y))
        });
        methods.add_meta_method(MetaMethod::Mul, |_, this, value: mlua::Value| match value {
            mlua::Value::Number(n) => {
                let n = n as f32;
                Ok(Vector2::new(this.x * n, this.y * n))
            }
            mlua::Value::Integer(n) => {
                let n = n as f32;
                Ok(Vector2::new(this.x * n, this.y * n))
            }
            mlua::Value::UserData(ud) => {
                let other = ud.borrow::<Vector2>()?;
                Ok(Vector2::new(this.x * other.x, this.y * other.y))
            }
            _ => Err(mlua::Error::runtime("Expected number or Vector2")),
        });
        methods.add_meta_method(MetaMethod::Div, |_, this, value: mlua::Value| match value {
            mlua::Value::Number(n) => {
                let n = n as f32;
                Ok(Vector2::new(this.x / n, this.y / n))
            }
            mlua::Value::Integer(n) => {
                let n = n as f32;
                Ok(Vector2::new(this.x / n, this.y / n))
            }
            mlua::Value::UserData(ud) => {
                let other = ud.borrow::<Vector2>()?;
                Ok(Vector2::new(this.x / other.x, this.y / other.y))
            }
            _ => Err(mlua::Error::runtime("Expected number or Vector2")),
        });
        methods.add_meta_method(MetaMethod::Unm, |_, this, ()| {
            Ok(Vector2::new(-this.x, -this.y))
        });
        methods.add_meta_method(MetaMethod::Eq, |_, this, other: Vector2| {
            Ok(this.x == other.x && this.y == other.y)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Vector2({}, {})", this.x, this.y))
        });
    }
}

pub fn register_vector2(lua: &Lua) -> Result<()> {
    let vector2_table = lua.create_table()?;

    vector2_table.set(
        "new",
        lua.create_function(|_, (x, y): (Option<f32>, Option<f32>)| {
            Ok(Vector2::new(x.unwrap_or(0.0), y.unwrap_or(0.0)))
        })?,
    )?;

    vector2_table.set("zero", Vector2::zero())?;
    vector2_table.set("one", Vector2::one())?;
    vector2_table.set("xAxis", Vector2::new(1.0, 0.0))?;
    vector2_table.set("yAxis", Vector2::new(0.0, 1.0))?;

    lua.globals().set("Vector2", vector2_table)?;

    Ok(())
}
