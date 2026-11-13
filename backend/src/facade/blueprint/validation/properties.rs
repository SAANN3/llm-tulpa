use super::{
    report::ValidationReport,
    values::{Shape, compatible},
};
use crate::facade::blueprint::catalog::ValueType;
use crate::tools::base::{PropertyInfo, PropertyType};
use std::collections::BTreeMap;

pub(super) struct Properties<'a> {
    pub report: &'a mut ValidationReport,
    pub node_id: Option<&'a str>,
}
impl Properties<'_> {
    fn error(&mut self, field: &str, code: &str, message: impl Into<String>) {
        self.report.error(self.node_id, field, code, message);
    }
    fn expect(&mut self, shape: &Shape, expected: ValueType, field: &str) {
        if !compatible(shape.kind, expected) {
            self.error(
                field,
                "type_mismatch",
                format!("expected {expected:?}, got {:?}", shape.kind),
            );
        }
    }
    pub fn property(&mut self, shape: &Shape, expected: &PropertyType, field: &str) {
        if shape.kind == ValueType::Dynamic {
            return;
        }
        let kind = match expected {
            PropertyType::String => ValueType::String,
            PropertyType::Number => ValueType::Number,
            PropertyType::Integer => ValueType::Integer,
            PropertyType::Boolean => ValueType::Boolean,
            PropertyType::Array { .. } => ValueType::Array,
            PropertyType::Object { .. } => ValueType::Object,
        };
        self.expect(shape, kind, field);
        match expected {
            PropertyType::Array { items } if shape.kind == ValueType::Array => {
                for (i, item) in shape.items.iter().enumerate() {
                    self.property(item, items, &format!("{field}[{i}]"));
                }
            }
            PropertyType::Object { properties } if shape.kind == ValueType::Object => {
                self.properties(&shape.fields, properties, field)
            }
            _ => {}
        }
    }
    pub fn properties(
        &mut self,
        fields: &BTreeMap<String, Shape>,
        properties: &[PropertyInfo],
        field: &str,
    ) {
        for property in properties {
            let path = format!("{field}.{}", property.name);
            if let Some(value) = fields.get(&property.name) {
                self.property(value, &property.property_type, &path);
            } else if property.required {
                self.error(&path, "missing_argument", "required argument is missing");
            }
        }
        // An empty object schema describes an open map in the existing tool schema contract.
        if !properties.is_empty() {
            for name in fields
                .keys()
                .filter(|name| !properties.iter().any(|p| p.name == **name))
            {
                self.error(
                    &format!("{field}.{name}"),
                    "unknown_argument",
                    "argument is not in the current tool schema",
                );
            }
        }
    }
}
