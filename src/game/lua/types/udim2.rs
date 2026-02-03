use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

use super::udim::UDim;

/// UDim2 represents 2D positioning with scale and offset for both X and Y axes
#[derive(Debug, Clone, Copy, Default)]
pub struct UDim2 {
    pub x: UDim,
    pub y: UDim,
}

impl UDim2 {
    pub fn new(x_scale: f32, x_offset: i32, y_scale: f32, y_offset: i32) -> Self {
        Self {
            x: UDim::new(x_scale, x_offset),
            y: UDim::new(y_scale, y_offset),
        }
    }

    pub fn from_scale(x_scale: f32, y_scale: f32) -> Self {
        Self::new(x_scale, 0, y_scale, 0)
    }

    pub fn from_offset(x_offset: i32, y_offset: i32) -> Self {
        Self::new(0.0, x_offset, 0.0, y_offset)
    }

    pub fn from_udims(x: UDim, y: UDim) -> Self {
        Self { x, y }
    }

    pub fn lerp(&self, goal: &UDim2, alpha: f32) -> Self {
        Self {
            x: UDim::new(
                self.x.scale + (goal.x.scale - self.x.scale) * alpha,
                self.x.offset + ((goal.x.offset - self.x.offset) as f32 * alpha) as i32,
            ),
            y: UDim::new(
                self.y.scale + (goal.y.scale - self.y.scale) * alpha,
                self.y.offset + ((goal.y.offset - self.y.offset) as f32 * alpha) as i32,
            ),
        }
    }
}

impl FromLua for UDim2 {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<UDim2>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "UDim2".to_string(),
                message: Some("expected UDim2".to_string()),
            }),
        }
    }
}

impl UserData for UDim2 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |_, this| Ok(this.x));
        fields.add_field_method_get("Y", |_, this| Ok(this.y));
        fields.add_field_method_get("Width", |_, this| Ok(this.x));
        fields.add_field_method_get("Height", |_, this| Ok(this.y));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Lerp", |_, this, (goal, alpha): (UDim2, f32)| {
            Ok(this.lerp(&goal, alpha))
        });

        methods.add_meta_method(MetaMethod::Add, |_, this, other: UDim2| {
            Ok(UDim2 {
                x: UDim::new(this.x.scale + other.x.scale, this.x.offset + other.x.offset),
                y: UDim::new(this.y.scale + other.y.scale, this.y.offset + other.y.offset),
            })
        });

        methods.add_meta_method(MetaMethod::Sub, |_, this, other: UDim2| {
            Ok(UDim2 {
                x: UDim::new(this.x.scale - other.x.scale, this.x.offset - other.x.offset),
                y: UDim::new(this.y.scale - other.y.scale, this.y.offset - other.y.offset),
            })
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: UDim2| {
            Ok(this.x.scale == other.x.scale
                && this.x.offset == other.x.offset
                && this.y.scale == other.y.scale
                && this.y.offset == other.y.offset)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "UDim2({{{}, {}}}, {{{}, {}}})",
                this.x.scale, this.x.offset, this.y.scale, this.y.offset
            ))
        });
    }
}

pub fn register_udim2(lua: &Lua) -> Result<()> {
    let udim2_table = lua.create_table()?;

    udim2_table.set(
        "new",
        lua.create_function(
            |_, (x_scale, x_offset, y_scale, y_offset): (f32, i32, f32, i32)| {
                Ok(UDim2::new(x_scale, x_offset, y_scale, y_offset))
            },
        )?,
    )?;

    udim2_table.set(
        "fromScale",
        lua.create_function(|_, (x_scale, y_scale): (f32, f32)| {
            Ok(UDim2::from_scale(x_scale, y_scale))
        })?,
    )?;

    udim2_table.set(
        "fromOffset",
        lua.create_function(|_, (x_offset, y_offset): (i32, i32)| {
            Ok(UDim2::from_offset(x_offset, y_offset))
        })?,
    )?;

    lua.globals().set("UDim2", udim2_table)?;

    Ok(())
}
