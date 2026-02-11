use mlua::{FromLua, Lua, Result, UserData, UserDataFields, Value};

use super::Vector3;

#[derive(Debug, Clone, Copy, Default)]
pub struct Ray {
    pub origin: Vector3,
    pub direction: Vector3,
}

impl Ray {
    pub fn new(origin: Vector3, direction: Vector3) -> Self {
        Self { origin, direction }
    }
}

impl FromLua for Ray {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Ray>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Ray".to_string(),
                message: Some("expected Ray".to_string()),
            }),
        }
    }
}

impl UserData for Ray {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Origin", |_, this| Ok(this.origin));
        fields.add_field_method_get("Direction", |_, this| Ok(this.direction));
    }
}

pub fn register_ray(lua: &Lua) -> Result<()> {
    let ray_table = lua.create_table()?;
    ray_table.set(
        "new",
        lua.create_function(|_, (origin, direction): (Vector3, Vector3)| {
            Ok(Ray::new(origin, direction))
        })?,
    )?;
    lua.globals().set("Ray", ray_table)?;
    Ok(())
}
