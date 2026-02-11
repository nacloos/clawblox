use mlua::{Lua, Result, Value};

use super::Color3;

fn color_from_name(name: &str) -> Color3 {
    let key = name.trim().to_ascii_lowercase();
    match key.as_str() {
        "medium stone grey" => Color3::from_rgb(163, 162, 165),
        "dark stone grey" => Color3::from_rgb(99, 95, 98),
        "bright yellow" => Color3::from_rgb(245, 205, 48),
        "bright blue" => Color3::from_rgb(13, 105, 172),
        "bright red" => Color3::from_rgb(196, 40, 28),
        "white" => Color3::from_rgb(242, 243, 243),
        "black" => Color3::from_rgb(27, 42, 53),
        _ => Color3::from_rgb(163, 162, 165),
    }
}

pub fn register_brick_color(lua: &Lua) -> Result<()> {
    let table = lua.create_table()?;

    table.set(
        "new",
        lua.create_function(|_, value: Value| -> Result<Color3> {
            match value {
                Value::String(s) => Ok(color_from_name(s.to_str()?.as_ref())),
                Value::Number(n) => {
                    // Minimal compatibility path for numeric constructors.
                    if (n as i64) % 2 == 0 {
                        Ok(Color3::from_rgb(163, 162, 165))
                    } else {
                        Ok(Color3::from_rgb(245, 205, 48))
                    }
                }
                Value::Integer(i) => {
                    if i % 2 == 0 {
                        Ok(Color3::from_rgb(163, 162, 165))
                    } else {
                        Ok(Color3::from_rgb(245, 205, 48))
                    }
                }
                Value::UserData(ud) => {
                    if let Ok(c) = ud.borrow::<Color3>() {
                        Ok(*c)
                    } else {
                        Err(mlua::Error::runtime("BrickColor.new expects string, number, or Color3"))
                    }
                }
                _ => Err(mlua::Error::runtime(
                    "BrickColor.new expects string, number, or Color3",
                )),
            }
        })?,
    )?;

    table.set(
        "Random",
        lua.create_function(|_, ()| Ok(Color3::from_rgb(163, 162, 165)))?,
    )?;

    lua.globals().set("BrickColor", table)?;
    Ok(())
}

