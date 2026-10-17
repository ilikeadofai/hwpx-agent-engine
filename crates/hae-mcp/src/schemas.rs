//! MCP schemas describe the response envelope, including execution errors.
use hae_exec::CommandDescriptor;
use serde_json::{json, Value};

/// Normalize schema-valued positions only. Never rewrite examples/default data.
/// Bare true becomes {}, and nullable type arrays become explicit anyOf branches.
pub fn portable_schema(schema: &mut Value) {
    if schema == &Value::Bool(true) || schema.as_object().is_some_and(|o| o.is_empty()) {
        // Explicit union preserves the any-JSON contract without portability
        // warnings for boolean schemas or object literals with no keywords.
        *schema = json!({"anyOf":[{"type":"object"},{"type":"array"},{"type":"string"},{"type":"number"},{"type":"boolean"},{"type":"null"}]});
        return;
    }
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    for key in [
        "properties",
        "$defs",
        "patternProperties",
        "dependentSchemas",
    ] {
        if let Some(values) = object.get_mut(key).and_then(Value::as_object_mut) {
            for child in values.values_mut() {
                portable_schema(child);
            }
        }
    }
    for key in [
        "items",
        "additionalProperties",
        "unevaluatedProperties",
        "contains",
        "not",
        "if",
        "then",
        "else",
        "propertyNames",
    ] {
        if let Some(child) = object.get_mut(key) {
            portable_schema(child);
        }
    }
    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(children) = object.get_mut(key).and_then(Value::as_array_mut) {
            for child in children {
                portable_schema(child);
            }
        }
    }
    if let Some(types) = object.get("type").and_then(Value::as_array).cloned() {
        object.remove("type");
        let branches =
            json!({"anyOf":types.into_iter().map(|t| json!({"type":t})).collect::<Vec<_>>()});
        // Preserve pre-existing composition constraints as an intersection.
        if object.contains_key("anyOf") {
            object
                .entry("allOf")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(branches);
        } else {
            object.insert("anyOf".into(), branches["anyOf"].clone());
        }
    }
}

pub fn output_schema(d: &CommandDescriptor) -> Value {
    let mut result = d.output_schema.clone();
    let definitions = result.as_object_mut().and_then(|o| {
        o.remove("$schema");
        o.remove("$defs")
    });
    let mut schema = json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object",
        "properties":{
            "schemaVersion":{"type":"integer","const":1},
            "sessionId":{"type":"string"},
            "requestId":{"type":"string"},
            "command":{"type":"string","const":d.name},
            "ok":{"type":"boolean"},
            "replayed":{"type":"boolean"},
            "result":result,
            "diagnostics":{"type":"array","items":{
                "type":"object",
                "properties":{
                    "code":{"type":"string"},"severity":{"type":"string"},
                    "stage":{"type":"string"},"message":{"type":"string"},
                    "target":{"type":"string"},"reason":{"type":"string"},"detail":{}
                },
                "required":["code","severity","stage","message"],
                "additionalProperties":false
            }}
        },
        "required":["schemaVersion","sessionId","command","ok","replayed"],
        "additionalProperties":false,
        "allOf":[{"if":{"properties":{"ok":{"const":true}}},"then":{"required":["result"]}}]
    });
    if let Some(definitions) = definitions {
        schema["$defs"] = definitions;
    }
    portable_schema(&mut schema);
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_keeps_default_data_and_nullability() {
        let mut schema = json!({"type":["string","null"],"default":true,"examples":[true],"properties":{"params":true}});
        portable_schema(&mut schema);
        assert_eq!(schema["anyOf"], json!([{"type":"string"},{"type":"null"}]));
        assert!(schema["properties"]["params"]["anyOf"].is_array());
        assert_eq!(schema["default"], true);
        assert_eq!(schema["examples"], json!([true]));
    }
}
