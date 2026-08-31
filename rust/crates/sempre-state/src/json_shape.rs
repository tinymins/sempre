use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
#[error("JSON shape mismatch at {path}: {message}")]
pub struct JsonShapeError {
    path: String,
    message: String,
}

pub fn validate_json_shape(actual: &Value, template: &Value) -> Result<(), JsonShapeError> {
    validate_at(actual, template, "$")
}

fn validate_at(actual: &Value, template: &Value, path: &str) -> Result<(), JsonShapeError> {
    match template {
        Value::Object(expected) if !expected.is_empty() => {
            let Value::Object(actual) = actual else {
                return mismatch(path, "expected an object");
            };
            for key in expected.keys() {
                if !actual.contains_key(key) {
                    return mismatch(path, format!("missing field {key:?}"));
                }
            }
            for key in actual.keys() {
                if !expected.contains_key(key) {
                    return mismatch(path, format!("unknown field {key:?}"));
                }
            }
            for (key, expected) in expected {
                validate_at(&actual[key], expected, &format!("{path}.{key}"))?;
            }
        }
        Value::Array(expected) if !expected.is_empty() => {
            let Value::Array(actual) = actual else {
                return mismatch(path, "expected an array");
            };
            for (index, value) in actual.iter().enumerate() {
                validate_at(value, &expected[0], &format!("{path}[{index}]"))?;
            }
        }
        Value::Object(_) if !actual.is_object() => return mismatch(path, "expected an object"),
        Value::Array(_) if !actual.is_array() => return mismatch(path, "expected an array"),
        _ => {}
    }
    Ok(())
}

fn mismatch(path: &str, message: impl Into<String>) -> Result<(), JsonShapeError> {
    Err(JsonShapeError {
        path: path.into(),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_exact_object_fields_and_repeated_array_item_shape() {
        let template = serde_json::json!({"items": [{"id": "", "enabled": false}]});
        validate_json_shape(
            &serde_json::json!({"items": [{"id": "one", "enabled": true}]}),
            &template,
        )
        .expect("matching shape");
        assert!(
            validate_json_shape(&serde_json::json!({"items": [{"id": "one"}]}), &template).is_err()
        );
        assert!(
            validate_json_shape(&serde_json::json!({"items": [], "extra": true}), &template)
                .is_err()
        );
    }
}
