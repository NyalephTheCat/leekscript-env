#[cfg(feature = "toml")]
pub fn toml_to_json(v: &toml::Value) -> serde_json::Value {
    match v {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => serde_json::Value::Object(
            t.iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect(),
        ),
    }
}

#[cfg(not(feature = "toml"))]
pub fn toml_to_json(_v: &()) -> serde_json::Value {
    serde_json::Value::Null
}

#[cfg(all(test, feature = "toml"))]
mod tests {
    use super::*;

    #[test]
    fn toml_to_json_basic_shapes() {
        let v: toml::Value = toml::from_str(
            r#"
title = "x"
nums = [1, 2]
[tbl]
k = "v"
"#,
        )
        .unwrap();
        let j = toml_to_json(&v);
        assert_eq!(j["title"], "x");
        assert_eq!(j["nums"][0], 1);
        assert_eq!(j["tbl"]["k"], "v");
    }
}
