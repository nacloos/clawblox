pub mod brick_color;
pub mod cframe;
pub mod color3;
pub mod datetime;
pub mod enums;
pub mod physical_properties;
pub mod ray;
pub mod udim;
pub mod udim2;
pub mod vector2;
pub mod vector3;

pub use cframe::CFrame;
pub use color3::Color3;
pub use datetime::DateTime;
pub use enums::{HumanoidStateType, Material, PartType, RaycastFilterType};
pub use physical_properties::PhysicalProperties;
pub use ray::Ray;
pub use udim::UDim;
pub use udim2::UDim2;
pub use vector2::Vector2;
pub use vector3::Vector3;

use mlua::{Lua, Result};

pub fn register_all_types(lua: &Lua) -> Result<()> {
    vector3::register_vector3(lua)?;
    vector2::register_vector2(lua)?;
    cframe::register_cframe(lua)?;
    color3::register_color3(lua)?;
    datetime::register_datetime(lua)?;
    brick_color::register_brick_color(lua)?;
    enums::register_enums(lua)?;
    physical_properties::register_physical_properties(lua)?;
    ray::register_ray(lua)?;
    udim::register_udim(lua)?;
    udim2::register_udim2(lua)?;
    Ok(())
}
