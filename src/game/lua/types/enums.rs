use mlua::{FromLua, Lua, Result, UserData, UserDataMethods, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartType {
    Ball,
    Block,
    Cylinder,
    Wedge,
}

impl PartType {
    pub fn name(&self) -> &'static str {
        match self {
            PartType::Ball => "Ball",
            PartType::Block => "Block",
            PartType::Cylinder => "Cylinder",
            PartType::Wedge => "Wedge",
        }
    }

    pub fn value(&self) -> i32 {
        match self {
            PartType::Ball => 0,
            PartType::Block => 1,
            PartType::Cylinder => 2,
            PartType::Wedge => 3,
        }
    }
}

impl FromLua for PartType {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<PartType>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "PartType".to_string(),
                message: Some("expected PartType".to_string()),
            }),
        }
    }
}

impl UserData for PartType {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Enum.PartType.{}", this.name()))
        });
        methods.add_meta_method(mlua::MetaMethod::Eq, |_, this, other: PartType| {
            Ok(*this == other)
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    Plastic,
    Wood,
    Metal,
    Glass,
    Neon,
    Concrete,
    Brick,
    Granite,
    Grass,
    Ice,
    Sand,
    Fabric,
    Marble,
    Slate,
    SmoothPlastic,
    ForceField,
}

impl Material {
    pub fn name(&self) -> &'static str {
        match self {
            Material::Plastic => "Plastic",
            Material::Wood => "Wood",
            Material::Metal => "Metal",
            Material::Glass => "Glass",
            Material::Neon => "Neon",
            Material::Concrete => "Concrete",
            Material::Brick => "Brick",
            Material::Granite => "Granite",
            Material::Grass => "Grass",
            Material::Ice => "Ice",
            Material::Sand => "Sand",
            Material::Fabric => "Fabric",
            Material::Marble => "Marble",
            Material::Slate => "Slate",
            Material::SmoothPlastic => "SmoothPlastic",
            Material::ForceField => "ForceField",
        }
    }

    pub fn value(&self) -> i32 {
        match self {
            Material::Plastic => 256,
            Material::Wood => 512,
            Material::Metal => 1040,
            Material::Glass => 1568,
            Material::Neon => 288,
            Material::Concrete => 816,
            Material::Brick => 848,
            Material::Granite => 880,
            Material::Grass => 1280,
            Material::Ice => 1536,
            Material::Sand => 1296,
            Material::Fabric => 1312,
            Material::Marble => 784,
            Material::Slate => 800,
            Material::SmoothPlastic => 272,
            Material::ForceField => 1584,
        }
    }
}

impl FromLua for Material {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Material>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Material".to_string(),
                message: Some("expected Material".to_string()),
            }),
        }
    }
}

impl UserData for Material {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Enum.Material.{}", this.name()))
        });
        methods.add_meta_method(mlua::MetaMethod::Eq, |_, this, other: Material| {
            Ok(*this == other)
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanoidStateType {
    Running,
    Jumping,
    Freefall,
    Dead,
    Physics,
    None,
}

impl HumanoidStateType {
    pub fn name(&self) -> &'static str {
        match self {
            HumanoidStateType::Running => "Running",
            HumanoidStateType::Jumping => "Jumping",
            HumanoidStateType::Freefall => "Freefall",
            HumanoidStateType::Dead => "Dead",
            HumanoidStateType::Physics => "Physics",
            HumanoidStateType::None => "None",
        }
    }
}

impl FromLua for HumanoidStateType {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<HumanoidStateType>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "HumanoidStateType".to_string(),
                message: Some("expected HumanoidStateType".to_string()),
            }),
        }
    }
}

impl UserData for HumanoidStateType {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Enum.HumanoidStateType.{}", this.name()))
        });
        methods.add_meta_method(
            mlua::MetaMethod::Eq,
            |_, this, other: HumanoidStateType| Ok(*this == other),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaycastFilterType {
    Include,
    Exclude,
}

impl RaycastFilterType {
    pub fn name(&self) -> &'static str {
        match self {
            RaycastFilterType::Include => "Include",
            RaycastFilterType::Exclude => "Exclude",
        }
    }
}

impl FromLua for RaycastFilterType {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<RaycastFilterType>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "RaycastFilterType".to_string(),
                message: Some("expected RaycastFilterType".to_string()),
            }),
        }
    }
}

impl UserData for RaycastFilterType {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Enum.RaycastFilterType.{}", this.name()))
        });
        methods.add_meta_method(
            mlua::MetaMethod::Eq,
            |_, this, other: RaycastFilterType| Ok(*this == other),
        );
    }
}

pub fn register_enums(lua: &Lua) -> Result<()> {
    let enum_table = lua.create_table()?;

    let part_type_table = lua.create_table()?;
    part_type_table.set("Ball", PartType::Ball)?;
    part_type_table.set("Block", PartType::Block)?;
    part_type_table.set("Cylinder", PartType::Cylinder)?;
    part_type_table.set("Wedge", PartType::Wedge)?;
    enum_table.set("PartType", part_type_table)?;

    let material_table = lua.create_table()?;
    material_table.set("Plastic", Material::Plastic)?;
    material_table.set("Wood", Material::Wood)?;
    material_table.set("Metal", Material::Metal)?;
    material_table.set("Glass", Material::Glass)?;
    material_table.set("Neon", Material::Neon)?;
    material_table.set("Concrete", Material::Concrete)?;
    material_table.set("Brick", Material::Brick)?;
    material_table.set("Granite", Material::Granite)?;
    material_table.set("Grass", Material::Grass)?;
    material_table.set("Ice", Material::Ice)?;
    material_table.set("Sand", Material::Sand)?;
    material_table.set("Fabric", Material::Fabric)?;
    material_table.set("Marble", Material::Marble)?;
    material_table.set("Slate", Material::Slate)?;
    material_table.set("SmoothPlastic", Material::SmoothPlastic)?;
    material_table.set("ForceField", Material::ForceField)?;
    enum_table.set("Material", material_table)?;

    let state_type_table = lua.create_table()?;
    state_type_table.set("Running", HumanoidStateType::Running)?;
    state_type_table.set("Jumping", HumanoidStateType::Jumping)?;
    state_type_table.set("Freefall", HumanoidStateType::Freefall)?;
    state_type_table.set("Dead", HumanoidStateType::Dead)?;
    state_type_table.set("Physics", HumanoidStateType::Physics)?;
    state_type_table.set("None", HumanoidStateType::None)?;
    enum_table.set("HumanoidStateType", state_type_table)?;

    let filter_type_table = lua.create_table()?;
    filter_type_table.set("Include", RaycastFilterType::Include)?;
    filter_type_table.set("Exclude", RaycastFilterType::Exclude)?;
    enum_table.set("RaycastFilterType", filter_type_table)?;

    lua.globals().set("Enum", enum_table)?;

    Ok(())
}
