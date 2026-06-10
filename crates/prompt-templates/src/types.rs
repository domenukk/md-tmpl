//! Frontmatter type declarations and validation.

use std::fmt;

use crate::value::Value;

/// Expected type of a template variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarType {
    /// `str` — expects a string value.
    Str,
    /// `bool` — expects a boolean value.
    Bool,
    /// `int` — expects an integer value.
    Int,
    /// `float` — expects a floating-point value.
    Float,
    /// `list<field = type, ...>` — required fields per item.
    List(Vec<VarDecl>),
    /// `dict<field = type, ...>` — required fields.
    Dict(Vec<VarDecl>),
    /// `enum<Option1, Option2, ...>` — expects one of these variants.
    Enum(Vec<VariantDecl>),
}

/// Write a comma-separated `name = type` field list.
fn fmt_fields(fields: &[VarDecl], f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (i, decl) in fields.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{} = {}", decl.name, decl.var_type)?;
    }
    Ok(())
}

impl fmt::Display for VarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str => f.write_str("str"),
            Self::Bool => f.write_str("bool"),
            Self::Int => f.write_str("int"),
            Self::Float => f.write_str("float"),
            Self::List(fields) => {
                write!(f, "list<")?;
                fmt_fields(fields, f)?;
                write!(f, ">")
            }
            Self::Dict(fields) => {
                write!(f, "dict<")?;
                fmt_fields(fields, f)?;
                write!(f, ">")
            }
            Self::Enum(variants) => {
                write!(f, "enum<")?;
                for (i, var) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", var.name)?;
                    if !var.fields.is_empty() {
                        write!(f, "(")?;
                        fmt_fields(&var.fields, f)?;
                        write!(f, ")")?;
                    }
                }
                write!(f, ">")
            }
        }
    }
}

impl VarType {
    /// Returns `true` if `value` is compatible with this declared type.
    ///
    /// - Scalar types match their corresponding `Value` variant.
    /// - `List(fields)` matches `Value::List`; if `fields` is non-empty,
    ///   **every** item must be a `Dict` with all required keys **and**
    ///   matching value types (recursive).
    /// - `Dict(fields)` matches `Value::Dict`; required keys must be present
    ///   with matching value types (recursive).
    /// - `Enum(variants)` matches unit variants as `Value::Str`, struct
    ///   variants as `Value::Dict` with `tag` + typed fields.
    #[must_use]
    pub fn matches(&self, value: &Value) -> bool {
        self.check(value).is_ok()
    }

    /// Validate `value` against this type, returning a structured error with
    /// the path to the first mismatch on failure.
    ///
    /// # Errors
    ///
    /// Returns [`TypeCheckError`] with the dotted path to the mismatched field,
    /// the expected type, the actual type, and a preview of the actual value.
    pub fn check(&self, value: &Value) -> Result<(), TypeCheckError> {
        self.check_inner(value, String::new())
    }

    fn check_inner(&self, value: &Value, path: String) -> Result<(), TypeCheckError> {
        match self {
            Self::Str => {
                if matches!(value, Value::Str(_)) {
                    Ok(())
                } else {
                    Err(TypeCheckError::new(path, "str", value))
                }
            }
            Self::Bool => {
                if matches!(value, Value::Bool(_)) {
                    Ok(())
                } else {
                    Err(TypeCheckError::new(path, "bool", value))
                }
            }
            Self::Int => {
                if matches!(value, Value::Int(_)) {
                    Ok(())
                } else {
                    Err(TypeCheckError::new(path, "int", value))
                }
            }
            Self::Float => {
                if matches!(value, Value::Float(_)) {
                    Ok(())
                } else {
                    Err(TypeCheckError::new(path, "float", value))
                }
            }
            Self::List(fields) => Self::check_list(fields, value, path),
            Self::Dict(fields) => Self::check_dict(fields, value, path),
            Self::Enum(variants) => Self::check_enum(variants, value, path),
        }
    }

    /// Validate a `List` type: each item must be a dict with all required fields.
    fn check_list(fields: &[VarDecl], value: &Value, path: String) -> Result<(), TypeCheckError> {
        let Value::List(items) = value else {
            return Err(TypeCheckError::new(path, "list", value));
        };
        if fields.is_empty() {
            return Ok(());
        }
        for (i, item) in items.iter().enumerate() {
            let Value::Dict(map) = item else {
                return Err(TypeCheckError::new(format!("{path}[{i}]"), "dict", item));
            };
            for decl in fields {
                let field_path = if path.is_empty() {
                    format!("[{i}].{}", decl.name)
                } else {
                    format!("{path}[{i}].{}", decl.name)
                };
                match map.get(&decl.name) {
                    Some(v) => decl.var_type.check_inner(v, field_path)?,
                    None => {
                        return Err(TypeCheckError {
                            path: field_path,
                            expected: decl.var_type.to_string(),
                            actual: "missing".into(),
                            actual_value: String::new(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate a `Dict` type: all required keys must be present with matching types.
    fn check_dict(fields: &[VarDecl], value: &Value, path: String) -> Result<(), TypeCheckError> {
        let Value::Dict(map) = value else {
            return Err(TypeCheckError::new(path, "dict", value));
        };
        for decl in fields {
            let field_path = if path.is_empty() {
                decl.name.clone()
            } else {
                format!("{path}.{}", decl.name)
            };
            match map.get(&decl.name) {
                Some(v) => decl.var_type.check_inner(v, field_path)?,
                None => {
                    return Err(TypeCheckError {
                        path: field_path,
                        expected: decl.var_type.to_string(),
                        actual: "missing".into(),
                        actual_value: String::new(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Validate an `Enum` type: unit variants match strings, struct variants
    /// match dicts with a `tag` field and typed fields.
    fn check_enum(
        variants: &[VariantDecl],
        value: &Value,
        path: String,
    ) -> Result<(), TypeCheckError> {
        match value {
            Value::Str(s) => {
                if variants.iter().any(|v| v.name == *s && v.fields.is_empty()) {
                    Ok(())
                } else {
                    let variant_names: Vec<&str> =
                        variants.iter().map(|v| v.name.as_str()).collect();
                    Err(TypeCheckError {
                        path,
                        expected: format!("enum<{}>", variant_names.join(", ")),
                        actual: format!("str({s})"),
                        actual_value: s.clone(),
                    })
                }
            }
            Value::Dict(map) => {
                let tag_key = crate::consts::ENUM_TAG_KEY;
                let Some(Value::Str(tag)) = map.get(tag_key) else {
                    return Err(TypeCheckError {
                        path,
                        expected: "enum dict with 'tag' field".into(),
                        actual: value.type_name().into(),
                        actual_value: value.to_string(),
                    });
                };
                let Some(var) = variants.iter().find(|v| v.name == *tag) else {
                    let variant_names: Vec<&str> =
                        variants.iter().map(|v| v.name.as_str()).collect();
                    return Err(TypeCheckError {
                        path: format!("{path}.tag"),
                        expected: format!("one of [{}]", variant_names.join(", ")),
                        actual: format!("'{tag}'"),
                        actual_value: tag.clone(),
                    });
                };
                for decl in &var.fields {
                    let field_path = if path.is_empty() {
                        decl.name.clone()
                    } else {
                        format!("{path}.{}", decl.name)
                    };
                    match map.get(&decl.name) {
                        Some(v) => decl.var_type.check_inner(v, field_path)?,
                        None => {
                            return Err(TypeCheckError {
                                path: field_path,
                                expected: decl.var_type.to_string(),
                                actual: "missing".into(),
                                actual_value: String::new(),
                            });
                        }
                    }
                }
                Ok(())
            }
            _ => Err(TypeCheckError::new(
                path,
                &VarType::Enum(variants.to_vec()).to_string(),
                value,
            )),
        }
    }
}

/// Structured error from [`VarType::check`] with the path to the mismatch.
#[derive(Debug, Clone)]
pub struct TypeCheckError {
    /// Dotted path to the mismatched field (e.g. `"bugs[2].title"`).
    pub path: String,
    /// The expected type at that path.
    pub expected: String,
    /// The actual type found.
    pub actual: String,
    /// Preview of the actual value.
    pub actual_value: String,
}

/// Maximum length for the actual-value preview in error messages.
const MAX_PREVIEW_LEN: usize = 60;

impl TypeCheckError {
    fn new(path: String, expected: &str, value: &Value) -> Self {
        let preview = value.to_string();
        let actual_value = if preview.len() > MAX_PREVIEW_LEN {
            // Truncate at a character boundary to avoid panicking on multi-byte UTF-8.
            let truncate_at = preview
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= MAX_PREVIEW_LEN - 3)
                .last()
                .unwrap_or(0);
            format!("{}…", &preview[..truncate_at])
        } else {
            preview
        };
        Self {
            path,
            expected: expected.into(),
            actual: value.type_name().into(),
            actual_value,
        }
    }
}

impl fmt::Display for TypeCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "expected {}, got {}", self.expected, self.actual)?;
        } else {
            write!(
                f,
                "at '{}': expected {}, got {}",
                self.path, self.expected, self.actual
            )?;
        }
        if !self.actual_value.is_empty() {
            write!(f, " ({})", self.actual_value)?;
        }
        Ok(())
    }
}

/// A variant declaration inside an enum type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDecl {
    /// Variant name.
    pub name: String,
    /// Optional associated fields for struct variants.
    pub fields: Vec<VarDecl>,
}

/// A variable declaration: name + type + optional default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDecl {
    /// Variable name.
    pub name: String,
    /// Expected type.
    pub var_type: VarType,
    /// Optional default value for this parameter.
    pub default_value: Option<crate::value::Value>,
}

impl VarDecl {
    /// Returns the default value for this declaration, if any.
    #[must_use]
    pub fn default_value(&self) -> Option<&crate::value::Value> {
        self.default_value.as_ref()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    // -- Display --

    #[test]
    fn display_scalar_types() {
        assert_eq!(VarType::Str.to_string(), "str");
        assert_eq!(VarType::Bool.to_string(), "bool");
        assert_eq!(VarType::Int.to_string(), "int");
        assert_eq!(VarType::Float.to_string(), "float");
    }

    #[test]
    fn display_list_with_fields() {
        let var_type = VarType::List(vec![
            VarDecl {
                name: "name".into(),
                var_type: VarType::Str,
                default_value: None,
            },
            VarDecl {
                name: "score".into(),
                var_type: VarType::Int,
                default_value: None,
            },
        ]);
        assert_eq!(var_type.to_string(), "list<name = str, score = int>");
    }

    #[test]
    fn display_dict_with_fields() {
        let var_type = VarType::Dict(vec![VarDecl {
            name: "label".into(),
            var_type: VarType::Str,
            default_value: None,
        }]);
        assert_eq!(var_type.to_string(), "dict<label = str>");
    }

    // -- matches --

    #[test]
    fn str_matches_str_only() {
        assert!(VarType::Str.matches(&Value::Str("hello".into())));
        assert!(!VarType::Str.matches(&Value::Bool(true)));
        assert!(!VarType::Str.matches(&Value::Int(1)));
    }

    #[test]
    fn bool_matches_bool_only() {
        assert!(VarType::Bool.matches(&Value::Bool(false)));
        assert!(!VarType::Bool.matches(&Value::Str("true".into())));
    }

    #[test]
    fn int_matches_int_only() {
        assert!(VarType::Int.matches(&Value::Int(42)));
        assert!(!VarType::Int.matches(&Value::Float(42.0)));
    }

    #[test]
    fn float_matches_float_only() {
        assert!(VarType::Float.matches(&Value::Float(3.25)));
        assert!(!VarType::Float.matches(&Value::Int(3)));
    }

    #[test]
    fn list_no_fields_matches_any_list() {
        assert!(VarType::List(vec![]).matches(&Value::List(vec![])));
        assert!(VarType::List(vec![]).matches(&Value::List(vec![Value::Int(1)])));
        assert!(!VarType::List(vec![]).matches(&Value::Str("x".into())));
    }

    #[test]
    fn list_with_fields_validates_all_items() {
        let var_type = VarType::List(vec![VarDecl {
            name: "name".into(),
            var_type: VarType::Str,
            default_value: None,
        }]);

        // Empty list passes (nothing to validate).
        assert!(var_type.matches(&Value::List(vec![])));

        // Single valid item.
        let valid_item = Value::Dict(HashMap::from([("name".into(), Value::Str("a".into()))]));
        assert!(var_type.matches(&Value::List(vec![valid_item])));

        // Missing key in first item.
        let invalid_item = Value::Dict(HashMap::from([("id".into(), Value::Int(1))]));
        assert!(!var_type.matches(&Value::List(vec![invalid_item])));

        // First item is not a Dict.
        assert!(!var_type.matches(&Value::List(vec![Value::Int(1)])));
    }

    #[test]
    fn list_with_fields_rejects_wrong_value_type() {
        let var_type = VarType::List(vec![VarDecl {
            name: "name".into(),
            var_type: VarType::Str,
            default_value: None,
        }]);

        // Key present but wrong type (Int instead of Str).
        let wrong_type = Value::Dict(HashMap::from([("name".into(), Value::Int(42))]));
        assert!(
            !var_type.matches(&Value::List(vec![wrong_type])),
            "should reject list item where 'name' is int, not str"
        );
    }

    #[test]
    fn list_validates_all_items_not_just_first() {
        let var_type = VarType::List(vec![VarDecl {
            name: "name".into(),
            var_type: VarType::Str,
            default_value: None,
        }]);

        let good = Value::Dict(HashMap::from([("name".into(), Value::Str("ok".into()))]));
        let bad = Value::Dict(HashMap::from([("name".into(), Value::Int(99))]));

        // First item good, second bad → reject.
        assert!(
            !var_type.matches(&Value::List(vec![good.clone(), bad])),
            "should validate ALL items, not just the first"
        );

        // Both good → accept.
        assert!(var_type.matches(&Value::List(vec![good.clone(), good])));
    }

    #[test]
    fn dict_validates_required_keys_and_types() {
        let var_type = VarType::Dict(vec![
            VarDecl {
                name: "title".into(),
                var_type: VarType::Str,
                default_value: None,
            },
            VarDecl {
                name: "count".into(),
                var_type: VarType::Int,
                default_value: None,
            },
        ]);

        let valid = Value::Dict(HashMap::from([
            ("title".into(), Value::Str("task".into())),
            ("count".into(), Value::Int(5)),
        ]));
        assert!(var_type.matches(&valid));

        // Missing "count".
        let missing_field =
            Value::Dict(HashMap::from([("title".into(), Value::Str("task".into()))]));
        assert!(!var_type.matches(&missing_field));

        // Not a dict at all.
        assert!(!var_type.matches(&Value::Str("oops".into())));
    }

    #[test]
    fn dict_rejects_wrong_field_type() {
        let var_type = VarType::Dict(vec![VarDecl {
            name: "count".into(),
            var_type: VarType::Int,
            default_value: None,
        }]);

        // Key present but wrong type (Str instead of Int).
        let wrong = Value::Dict(HashMap::from([("count".into(), Value::Str("five".into()))]));
        assert!(
            !var_type.matches(&wrong),
            "should reject dict where 'count' is str, not int"
        );
    }

    #[test]
    fn dict_nested_type_checking() {
        // dict<meta = dict<version = int>>
        let var_type = VarType::Dict(vec![VarDecl {
            name: "meta".into(),
            var_type: VarType::Dict(vec![VarDecl {
                name: "version".into(),
                var_type: VarType::Int,
                default_value: None,
            }]),
            default_value: None,
        }]);

        let valid = Value::Dict(HashMap::from([(
            "meta".into(),
            Value::Dict(HashMap::from([("version".into(), Value::Int(3))])),
        )]));
        assert!(var_type.matches(&valid));

        // Nested field wrong type.
        let wrong = Value::Dict(HashMap::from([(
            "meta".into(),
            Value::Dict(HashMap::from([("version".into(), Value::Str("3".into()))])),
        )]));
        assert!(
            !var_type.matches(&wrong),
            "should recursively check nested dict field types"
        );
    }

    #[test]
    fn dict_no_fields_matches_any_dict() {
        assert!(VarType::Dict(vec![]).matches(&Value::Dict(HashMap::new())));
        assert!(!VarType::Dict(vec![]).matches(&Value::List(vec![])));
    }

    #[test]
    fn display_enum_with_fields() {
        let var_type = VarType::Enum(vec![
            VariantDecl {
                name: "Confirmed".into(),
                fields: vec![VarDecl {
                    name: "evidence".into(),
                    var_type: VarType::Str,
                    default_value: None,
                }],
            },
            VariantDecl {
                name: "Inconclusive".into(),
                fields: vec![],
            },
        ]);
        assert_eq!(
            var_type.to_string(),
            "enum<Confirmed(evidence = str), Inconclusive>"
        );
    }

    #[test]
    fn enum_matches_validation() {
        let var_type = VarType::Enum(vec![
            VariantDecl {
                name: "Confirmed".into(),
                fields: vec![VarDecl {
                    name: "evidence".into(),
                    var_type: VarType::Str,
                    default_value: None,
                }],
            },
            VariantDecl {
                name: "Inconclusive".into(),
                fields: vec![],
            },
        ]);

        // String value matching unit variant
        assert!(var_type.matches(&Value::Str("Inconclusive".into())));
        assert!(!var_type.matches(&Value::Str("Confirmed".into())));

        // Internally tagged dict matching struct variant
        let valid_dict = Value::Dict(HashMap::from([
            ("tag".into(), Value::Str("Confirmed".into())),
            ("evidence".into(), Value::Str("some evidence".into())),
        ]));
        assert!(var_type.matches(&valid_dict));

        // Missing required field
        let missing_field = Value::Dict(HashMap::from([(
            "tag".into(),
            Value::Str("Confirmed".into()),
        )]));
        assert!(!var_type.matches(&missing_field));

        // Invalid variant name
        let invalid_variant = Value::Dict(HashMap::from([(
            "tag".into(),
            Value::Str("Unknown".into()),
        )]));
        assert!(!var_type.matches(&invalid_variant));
    }

    #[test]
    fn enum_rejects_wrong_field_type() {
        let var_type = VarType::Enum(vec![VariantDecl {
            name: "Confirmed".into(),
            fields: vec![VarDecl {
                name: "evidence".into(),
                var_type: VarType::Str,
                default_value: None,
            }],
        }]);

        // Field present but wrong type (Int instead of Str).
        let wrong = Value::Dict(HashMap::from([
            ("tag".into(), Value::Str("Confirmed".into())),
            ("evidence".into(), Value::Int(42)),
        ]));
        assert!(
            !var_type.matches(&wrong),
            "should reject enum variant where 'evidence' is int, not str"
        );
    }

    // -- check() path diagnostics --

    #[test]
    fn check_scalar_error_has_empty_path() {
        let err = VarType::Int.check(&Value::Str("oops".into())).unwrap_err();
        assert!(
            err.path.is_empty(),
            "scalar mismatch should have empty path"
        );
        assert_eq!(err.expected, "int");
        assert_eq!(err.actual, "str");
    }

    #[test]
    fn check_list_item_field_path() {
        let var_type = VarType::List(vec![VarDecl {
            name: "score".into(),
            var_type: VarType::Int,
            default_value: None,
        }]);
        // Second item has wrong type for score.
        let items = Value::List(vec![
            Value::Dict(HashMap::from([("score".into(), Value::Int(10))])),
            Value::Dict(HashMap::from([("score".into(), Value::Str("bad".into()))])),
        ]);
        let err = var_type.check(&items).unwrap_err();
        assert_eq!(err.path, "[1].score", "should point to items[1].score");
        assert_eq!(err.expected, "int");
    }

    #[test]
    fn check_dict_missing_field_path() {
        let var_type = VarType::Dict(vec![VarDecl {
            name: "title".into(),
            var_type: VarType::Str,
            default_value: None,
        }]);
        let value = Value::Dict(HashMap::new()); // missing 'title'
        let err = var_type.check(&value).unwrap_err();
        assert_eq!(err.path, "title");
        assert_eq!(err.actual, "missing");
    }

    #[test]
    fn check_nested_dict_path() {
        let var_type = VarType::Dict(vec![VarDecl {
            name: "meta".into(),
            var_type: VarType::Dict(vec![VarDecl {
                name: "version".into(),
                var_type: VarType::Int,
                default_value: None,
            }]),
            default_value: None,
        }]);
        let value = Value::Dict(HashMap::from([(
            "meta".into(),
            Value::Dict(HashMap::from([("version".into(), Value::Str("3".into()))])),
        )]));
        let err = var_type.check(&value).unwrap_err();
        assert_eq!(err.path, "meta.version", "should show nested path");
    }

    #[test]
    fn check_enum_invalid_tag_path() {
        let var_type = VarType::Enum(vec![VariantDecl {
            name: "Confirmed".into(),
            fields: vec![],
        }]);
        let value = Value::Dict(HashMap::from([(
            "tag".into(),
            Value::Str("Unknown".into()),
        )]));
        let err = var_type.check(&value).unwrap_err();
        assert_eq!(err.path, ".tag");
    }

    #[test]
    fn check_display_with_path() {
        let err = TypeCheckError {
            path: "bugs[2].title".into(),
            expected: "str".into(),
            actual: "int".into(),
            actual_value: "42".into(),
        };
        assert_eq!(
            err.to_string(),
            "at 'bugs[2].title': expected str, got int (42)"
        );
    }

    #[test]
    fn check_display_empty_path() {
        let err = TypeCheckError {
            path: String::new(),
            expected: "str".into(),
            actual: "int".into(),
            actual_value: "42".into(),
        };
        assert_eq!(err.to_string(), "expected str, got int (42)");
    }
}
