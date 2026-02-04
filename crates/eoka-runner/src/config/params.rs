use crate::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;

/// Runtime parameters passed to a config.
#[derive(Debug, Clone, Default)]
pub struct Params {
    values: HashMap<String, String>,
}

impl Params {
    /// Create empty params.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a parameter value.
    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.values.insert(key.into(), value.into());
        self
    }

    /// Get a parameter value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Parse from CLI args like "key=value".
    pub fn from_args(args: &[String]) -> Result<Self> {
        let mut params = Self::new();
        for arg in args {
            let (key, value) = arg.split_once('=').ok_or_else(|| {
                Error::Config(format!("invalid param '{}', expected key=value", arg))
            })?;
            params.values.insert(key.to_string(), value.to_string());
        }
        Ok(params)
    }
}

/// Parameter definition in config.
#[derive(Debug, Clone, Deserialize)]
pub struct ParamDef {
    /// Whether this parameter is required.
    #[serde(default)]
    pub required: bool,

    /// Default value if not provided.
    pub default: Option<String>,

    /// Description for documentation.
    pub description: Option<String>,
}

/// Substitute `${var}` patterns in a string.
pub fn substitute(
    template: &str,
    params: &Params,
    defs: &HashMap<String, ParamDef>,
) -> Result<String> {
    let mut result = template.to_string();
    let mut start = 0;

    while let Some(var_start) = result[start..].find("${") {
        let var_start = start + var_start;
        let Some(var_end) = result[var_start..].find('}') else {
            break;
        };
        let var_end = var_start + var_end;

        let var_name = &result[var_start + 2..var_end];

        let value = if let Some(v) = params.get(var_name) {
            v.to_string()
        } else if let Some(def) = defs.get(var_name) {
            if let Some(ref default) = def.default {
                default.clone()
            } else if def.required {
                return Err(Error::Config(format!(
                    "missing required parameter: {}",
                    var_name
                )));
            } else {
                // Optional param with no default - leave empty
                String::new()
            }
        } else {
            // Unknown param - leave as-is for now (might be env var or other substitution)
            start = var_end + 1;
            continue;
        };

        result.replace_range(var_start..=var_end, &value);
        start = var_start + value.len();
    }

    Ok(result)
}

/// Recursively substitute params in a serde_yaml::Value.
pub fn substitute_value(
    value: &mut serde_yaml::Value,
    params: &Params,
    defs: &HashMap<String, ParamDef>,
) -> Result<()> {
    match value {
        serde_yaml::Value::String(s) => {
            *s = substitute(s, params, defs)?;
        }
        serde_yaml::Value::Mapping(map) => {
            for (_, v) in map.iter_mut() {
                substitute_value(v, params, defs)?;
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for v in seq.iter_mut() {
                substitute_value(v, params, defs)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_simple() {
        let params = Params::new().set("name", "world");
        let defs = HashMap::new();
        let result = substitute("hello ${name}!", &params, &defs).unwrap();
        assert_eq!(result, "hello world!");
    }

    #[test]
    fn test_substitute_multiple() {
        let params = Params::new().set("a", "1").set("b", "2");
        let defs = HashMap::new();
        let result = substitute("${a} + ${b} = 3", &params, &defs).unwrap();
        assert_eq!(result, "1 + 2 = 3");
    }

    #[test]
    fn test_substitute_default() {
        let params = Params::new();
        let mut defs = HashMap::new();
        defs.insert(
            "name".to_string(),
            ParamDef {
                required: false,
                default: Some("default".to_string()),
                description: None,
            },
        );
        let result = substitute("hello ${name}", &params, &defs).unwrap();
        assert_eq!(result, "hello default");
    }

    #[test]
    fn test_substitute_required_missing() {
        let params = Params::new();
        let mut defs = HashMap::new();
        defs.insert(
            "name".to_string(),
            ParamDef {
                required: true,
                default: None,
                description: None,
            },
        );
        let result = substitute("hello ${name}", &params, &defs);
        assert!(result.is_err());
    }

    #[test]
    fn test_params_from_args() {
        let args = vec!["user=alice".to_string(), "pass=secret".to_string()];
        let params = Params::from_args(&args).unwrap();
        assert_eq!(params.get("user"), Some("alice"));
        assert_eq!(params.get("pass"), Some("secret"));
    }
}
