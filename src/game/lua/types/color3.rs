use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};

#[derive(Debug, Clone, Copy, Default)]
pub struct Color3 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3 {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
        }
    }

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(1.0);
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);

        let c = v * s;
        let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
        let m = v - c;

        let (r, g, b) = match (h * 6.0).floor() as i32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        Self::new(r + m, g + m, b + m)
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

        Some(Self::from_rgb(r, g, b))
    }

    pub fn to_hsv(&self) -> (f32, f32, f32) {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let delta = max - min;

        let h = if delta == 0.0 {
            0.0
        } else if max == self.r {
            ((self.g - self.b) / delta).rem_euclid(6.0) / 6.0
        } else if max == self.g {
            ((self.b - self.r) / delta + 2.0) / 6.0
        } else {
            ((self.r - self.g) / delta + 4.0) / 6.0
        };

        let s = if max == 0.0 { 0.0 } else { delta / max };
        let v = max;

        (h, s, v)
    }

    pub fn to_hex(&self) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8
        )
    }

    pub fn lerp(&self, goal: &Color3, alpha: f32) -> Self {
        Self::new(
            self.r + (goal.r - self.r) * alpha,
            self.g + (goal.g - self.g) * alpha,
            self.b + (goal.b - self.b) * alpha,
        )
    }
}

impl FromLua for Color3 {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Color3>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Color3".to_string(),
                message: Some("expected Color3".to_string()),
            }),
        }
    }
}

impl UserData for Color3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("R", |_, this| Ok(this.r));
        fields.add_field_method_get("G", |_, this| Ok(this.g));
        fields.add_field_method_get("B", |_, this| Ok(this.b));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Lerp", |_, this, (goal, alpha): (Color3, f32)| {
            Ok(this.lerp(&goal, alpha))
        });
        methods.add_method("ToHSV", |_, this, ()| {
            let (h, s, v) = this.to_hsv();
            Ok((h, s, v))
        });
        methods.add_method("ToHex", |_, this, ()| Ok(this.to_hex()));

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: Color3| {
            Ok(this.r == other.r && this.g == other.g && this.b == other.b)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Color3({}, {}, {})", this.r, this.g, this.b))
        });
    }
}

pub fn register_color3(lua: &Lua) -> Result<()> {
    let color3_table = lua.create_table()?;

    color3_table.set(
        "new",
        lua.create_function(|_, (r, g, b): (Option<f32>, Option<f32>, Option<f32>)| {
            Ok(Color3::new(
                r.unwrap_or(0.0),
                g.unwrap_or(0.0),
                b.unwrap_or(0.0),
            ))
        })?,
    )?;

    color3_table.set(
        "fromRGB",
        lua.create_function(|_, (r, g, b): (u8, u8, u8)| Ok(Color3::from_rgb(r, g, b)))?,
    )?;

    color3_table.set(
        "fromHSV",
        lua.create_function(|_, (h, s, v): (f32, f32, f32)| Ok(Color3::from_hsv(h, s, v)))?,
    )?;

    color3_table.set(
        "fromHex",
        lua.create_function(|_, hex: String| {
            Color3::from_hex(&hex).ok_or_else(|| mlua::Error::runtime("Invalid hex color"))
        })?,
    )?;

    lua.globals().set("Color3", color3_table)?;

    Ok(())
}
