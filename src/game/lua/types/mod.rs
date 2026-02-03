pub mod cframe;
pub mod color3;
pub mod enums;
pub mod udim;
pub mod udim2;
pub mod vector3;

pub use cframe::CFrame;
pub use color3::Color3;
pub use enums::{Material, PartType, RaycastFilterType};
pub use udim::UDim;
pub use udim2::UDim2;
pub use vector3::Vector3;

use mlua::{Lua, Result};

pub fn register_all_types(lua: &Lua) -> Result<()> {
    vector3::register_vector3(lua)?;
    cframe::register_cframe(lua)?;
    color3::register_color3(lua)?;
    enums::register_enums(lua)?;
    udim::register_udim(lua)?;
    udim2::register_udim2(lua)?;
    Ok(())
}
