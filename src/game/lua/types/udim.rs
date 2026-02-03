use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

/// UDim represents a single dimension with scale (0-1 fraction) and offset (pixels)
#[derive(Debug, Clone, Copy, Default)]
pub struct UDim {
    pub scale: f32,
    pub offset: i32,
}

impl UDim {
    pub fn new(scale: f32, offset: i32) -> Self {
        Self { scale, offset }
    }
}

impl FromLua for UDim {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<UDim>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "UDim".to_string(),
                message: Some("expected UDim".to_string()),
            }),
        }
    }
}

impl UserData for UDim {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Scale", |_, this| Ok(this.scale));
        fields.add_field_method_get("Offset", |_, this| Ok(this.offset));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Add, |_, this, other: UDim| {
            Ok(UDim::new(this.scale + other.scale, this.offset + other.offset))
        });

        methods.add_meta_method(MetaMethod::Sub, |_, this, other: UDim| {
            Ok(UDim::new(this.scale - other.scale, this.offset - other.offset))
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: UDim| {
            Ok(this.scale == other.scale && this.offset == other.offset)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("UDim({}, {})", this.scale, this.offset))
        });
    }
}

pub fn register_udim(lua: &Lua) -> Result<()> {
    let udim_table = lua.create_table()?;

    udim_table.set(
        "new",
        lua.create_function(|_, (scale, offset): (f32, i32)| Ok(UDim::new(scale, offset)))?,
    )?;

    lua.globals().set("UDim", udim_table)?;

    Ok(())
}
