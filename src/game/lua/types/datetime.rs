use chrono::{DateTime as ChronoDateTime, Local, TimeZone, Timelike, Utc};
use mlua::{FromLua, Lua, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Default)]
pub struct DateTime {
    unix_timestamp_millis: i64,
}

impl DateTime {
    pub fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            unix_timestamp_millis: millis,
        }
    }

    pub fn from_unix_timestamp(seconds: i64) -> Self {
        Self {
            unix_timestamp_millis: seconds.saturating_mul(1000),
        }
    }

    pub fn from_unix_timestamp_millis(millis: i64) -> Self {
        Self {
            unix_timestamp_millis: millis,
        }
    }

    pub fn unix_timestamp_millis(&self) -> i64 {
        self.unix_timestamp_millis
    }

    pub fn unix_timestamp(&self) -> i64 {
        self.unix_timestamp_millis.div_euclid(1000)
    }

    pub fn to_iso_date(&self) -> String {
        if let Some(dt) = ChronoDateTime::<Utc>::from_timestamp_millis(self.unix_timestamp_millis) {
            dt.to_rfc3339()
        } else {
            "1970-01-01T00:00:00+00:00".to_string()
        }
    }

    pub fn to_local_time(&self) -> Self {
        *self
    }

    pub fn to_universal_time(&self) -> Self {
        *self
    }

    pub fn format_local_time(&self, format: &str, _locale: &str) -> String {
        if let Some(dt_utc) = ChronoDateTime::<Utc>::from_timestamp_millis(self.unix_timestamp_millis)
        {
            dt_utc.with_timezone(&Local).format(format).to_string()
        } else {
            "".to_string()
        }
    }

    pub fn format_universal_time(&self, format: &str, _locale: &str) -> String {
        if let Some(dt_utc) = ChronoDateTime::<Utc>::from_timestamp_millis(self.unix_timestamp_millis)
        {
            dt_utc.format(format).to_string()
        } else {
            "".to_string()
        }
    }
}

impl FromLua for DateTime {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<DateTime>().map(|v| *v),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "DateTime".to_string(),
                message: Some("expected DateTime".to_string()),
            }),
        }
    }
}

impl UserData for DateTime {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("UnixTimestamp", |_, this| Ok(this.unix_timestamp()));
        fields.add_field_method_get("UnixTimestampMillis", |_, this| Ok(this.unix_timestamp_millis()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("ToIsoDate", |_, this, ()| Ok(this.to_iso_date()));
        methods.add_method("ToLocalTime", |_, this, ()| Ok(this.to_local_time()));
        methods.add_method("ToUniversalTime", |_, this, ()| Ok(this.to_universal_time()));
        methods.add_method(
            "FormatLocalTime",
            |_, this, (format, locale): (String, Option<String>)| {
                Ok(this.format_local_time(&format, locale.as_deref().unwrap_or("en-us")))
            },
        );
        methods.add_method(
            "FormatUniversalTime",
            |_, this, (format, locale): (String, Option<String>)| {
                Ok(this.format_universal_time(
                    &format,
                    locale.as_deref().unwrap_or("en-us"),
                ))
            },
        );

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: DateTime| {
            Ok(this.unix_timestamp_millis == other.unix_timestamp_millis)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("DateTime({})", this.to_iso_date()))
        });
    }
}

pub fn register_datetime(lua: &Lua) -> Result<()> {
    let datetime_table = lua.create_table()?;

    datetime_table.set("now", lua.create_function(|_, ()| Ok(DateTime::now()))?)?;

    datetime_table.set(
        "fromUnixTimestamp",
        lua.create_function(|_, seconds: i64| Ok(DateTime::from_unix_timestamp(seconds)))?,
    )?;

    datetime_table.set(
        "fromUnixTimestampMillis",
        lua.create_function(|_, millis: i64| Ok(DateTime::from_unix_timestamp_millis(millis)))?,
    )?;

    datetime_table.set(
        "fromUniversalTime",
        lua.create_function(
            |_,
             (year, month, day, hour, minute, second, millisecond): (
                i32,
                u32,
                u32,
                Option<u32>,
                Option<u32>,
                Option<u32>,
                Option<u32>,
            )| {
                let h = hour.unwrap_or(0);
                let m = minute.unwrap_or(0);
                let s = second.unwrap_or(0);
                let ms = millisecond.unwrap_or(0);
                let maybe = Utc
                    .with_ymd_and_hms(year, month, day, h, m, s)
                    .single()
                    .and_then(|dt| dt.with_nanosecond(ms.saturating_mul(1_000_000)));
                match maybe {
                    Some(dt) => Ok(DateTime::from_unix_timestamp_millis(dt.timestamp_millis())),
                    None => Err(mlua::Error::runtime("Invalid date components")),
                }
            },
        )?,
    )?;

    lua.globals().set("DateTime", datetime_table)?;
    Ok(())
}
