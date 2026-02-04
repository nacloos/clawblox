use mlua::{UserData, UserDataFields, UserDataMethods, Value, Result as LuaResult, Lua};

/// HttpService provides JSON encoding/decoding utilities
#[derive(Clone)]
pub struct HttpService;

impl HttpService {
    pub fn new() -> Self {
        Self
    }

    /// Convert a Lua value to a serde_json::Value
    fn lua_to_json(value: &Value) -> serde_json::Value {
        match value {
            Value::Nil => serde_json::Value::Null,
            Value::Boolean(b) => serde_json::Value::Bool(*b),
            Value::Integer(i) => serde_json::Value::Number((*i).into()),
            Value::Number(n) => {
                serde_json::Number::from_f64(*n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
            Value::String(s) => serde_json::Value::String(s.to_str().map(|s| s.to_string()).unwrap_or_default()),
            Value::Table(t) => {
                // Check if it's an array (sequential integer keys starting at 1)
                let mut is_array = true;
                let mut max_index = 0;

                for pair in t.clone().pairs::<Value, Value>() {
                    if let Ok((key, _)) = pair {
                        match key {
                            Value::Integer(i) if i >= 1 => {
                                max_index = max_index.max(i as usize);
                            }
                            _ => {
                                is_array = false;
                                break;
                            }
                        }
                    }
                }

                if is_array && max_index > 0 {
                    // It's an array
                    let mut arr = Vec::with_capacity(max_index);
                    for i in 1..=max_index {
                        if let Ok(val) = t.get::<Value>(i) {
                            arr.push(Self::lua_to_json(&val));
                        } else {
                            arr.push(serde_json::Value::Null);
                        }
                    }
                    serde_json::Value::Array(arr)
                } else {
                    // It's an object
                    let mut map = serde_json::Map::new();
                    for pair in t.clone().pairs::<Value, Value>() {
                        if let Ok((key, val)) = pair {
                            let key_str = match key {
                                Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                                Value::Integer(i) => i.to_string(),
                                Value::Number(n) => n.to_string(),
                                _ => continue,
                            };
                            map.insert(key_str, Self::lua_to_json(&val));
                        }
                    }
                    serde_json::Value::Object(map)
                }
            }
            _ => serde_json::Value::Null, // Unsupported types become null
        }
    }

    /// Convert a serde_json::Value to a Lua value
    fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
        match value {
            serde_json::Value::Null => Ok(Value::Nil),
            serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Number(f))
                } else {
                    Ok(Value::Nil)
                }
            }
            serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
            serde_json::Value::Array(arr) => {
                let table = lua.create_table()?;
                for (i, val) in arr.iter().enumerate() {
                    table.set(i + 1, Self::json_to_lua(lua, val)?)?;
                }
                Ok(Value::Table(table))
            }
            serde_json::Value::Object(map) => {
                let table = lua.create_table()?;
                for (key, val) in map {
                    table.set(key.as_str(), Self::json_to_lua(lua, val)?)?;
                }
                Ok(Value::Table(table))
            }
        }
    }
}

impl Default for HttpService {
    fn default() -> Self {
        Self::new()
    }
}

impl UserData for HttpService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, _| Ok("HttpService".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("HttpService".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("JSONEncode", |_, _, value: Value| {
            let json = HttpService::lua_to_json(&value);
            Ok(serde_json::to_string(&json).unwrap_or_else(|_| "null".to_string()))
        });

        methods.add_method("JSONDecode", |lua, _, json_str: String| {
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(json) => HttpService::json_to_lua(lua, &json),
                Err(_) => Ok(Value::Nil),
            }
        });
    }
}
