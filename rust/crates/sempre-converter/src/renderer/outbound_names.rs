use serde_json::Value;

pub(super) const DIRECT: &str = crate::DIRECT_OUTBOUND_NAME;

pub(super) fn restore_direct(reference: &mut Value) {
    if reference.as_str() == Some("direct") {
        *reference = Value::from(DIRECT);
    } else if let Some(references) = reference.as_array_mut() {
        for item in references {
            if item.as_str() == Some("direct") {
                *item = Value::from(DIRECT);
            }
        }
    }
}

pub(super) fn restore_field(value: &mut Value, field: &str) {
    if let Some(reference) = value.get_mut(field) {
        restore_direct(reference);
    }
}
