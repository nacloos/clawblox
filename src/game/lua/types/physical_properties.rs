use mlua::{FromLua, Lua, Result, UserData, UserDataFields, UserDataMethods, Value};

#[derive(Debug, Clone, Copy, Default)]
pub struct PhysicalProperties {
    pub density: f32,
    pub friction: f32,
    pub elasticity: f32,
    pub friction_weight: f32,
    pub elasticity_weight: f32,
}

impl PhysicalProperties {
    pub fn new(
        density: f32,
        friction: f32,
        elasticity: f32,
        friction_weight: f32,
        elasticity_weight: f32,
    ) -> Self {
        Self {
            density,
            friction,
            elasticity,
            friction_weight,
            elasticity_weight,
        }
    }
}

impl FromLua for PhysicalProperties {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<PhysicalProperties>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "PhysicalProperties".to_string(),
                message: Some("expected PhysicalProperties".to_string()),
            }),
        }
    }
}

impl UserData for PhysicalProperties {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Density", |_, this| Ok(this.density));
        fields.add_field_method_get("Friction", |_, this| Ok(this.friction));
        fields.add_field_method_get("Elasticity", |_, this| Ok(this.elasticity));
        fields.add_field_method_get("FrictionWeight", |_, this| Ok(this.friction_weight));
        fields.add_field_method_get("ElasticityWeight", |_, this| Ok(this.elasticity_weight));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "PhysicalProperties({}, {}, {}, {}, {})",
                this.density,
                this.friction,
                this.elasticity,
                this.friction_weight,
                this.elasticity_weight
            ))
        });
    }
}

pub fn register_physical_properties(lua: &Lua) -> Result<()> {
    let table = lua.create_table()?;
    table.set(
        "new",
        lua.create_function(
            |_,
             (density, friction, elasticity, friction_weight, elasticity_weight): (
                Option<f32>,
                Option<f32>,
                Option<f32>,
                Option<f32>,
                Option<f32>,
            )| {
                Ok(PhysicalProperties::new(
                    density.unwrap_or(0.7),
                    friction.unwrap_or(0.3),
                    elasticity.unwrap_or(0.5),
                    friction_weight.unwrap_or(1.0),
                    elasticity_weight.unwrap_or(1.0),
                ))
            },
        )?,
    )?;

    lua.globals().set("PhysicalProperties", table)?;
    Ok(())
}
