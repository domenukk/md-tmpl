use serde::{Deserialize, Serialize};

use super::*;

#[test]
fn struct_to_dict() {
    #[derive(Serialize)]
    struct Agent {
        name: String,
        score: i64,
    }
    let agent = Agent {
        name: "Alice".into(),
        score: 95,
    };
    let val = to_value(&agent).unwrap();
    assert_eq!(val.get_field("name").unwrap().to_string(), "Alice");
    assert_eq!(*val.get_field("score").unwrap(), Value::Int(95));
}

#[test]
fn vec_to_list() {
    let items = vec!["alpha", "beta", "gamma"];
    let val = to_value(&items).unwrap();
    match &val {
        Value::List(v) => assert_eq!(v.len(), 3),
        other => panic!("expected List, got {}", other.type_name()),
    }
}

#[test]
fn nested_structs() {
    #[derive(Serialize)]
    struct Inner {
        label: String,
    }
    #[derive(Serialize)]
    struct Outer {
        items: Vec<Inner>,
        active: bool,
    }
    let data = Outer {
        items: vec![Inner { label: "a".into() }, Inner { label: "b".into() }],
        active: true,
    };
    let val = to_value(&data).unwrap();
    assert_eq!(*val.get_field("active").unwrap(), Value::Bool(true));
    match val.get_field("items").unwrap() {
        Value::List(v) => assert_eq!(v.len(), 2),
        other => panic!("expected List, got {}", other.type_name()),
    }
}

#[test]
fn hashmap_to_dict() {
    let mut map = HashMap::new();
    map.insert("key".to_string(), 42_i64);
    let val = to_value(&map).unwrap();
    assert_eq!(*val.get_field("key").unwrap(), Value::Int(42));
}

#[test]
fn option_some() {
    let val = to_value(&Some("hello")).unwrap();
    assert_eq!(val, Value::Str("hello".into()));
}

#[test]
fn option_none() {
    let val = to_value(&Option::<String>::None).unwrap();
    assert_eq!(val, Value::None);
}

#[test]
fn enum_unit_variant() {
    #[derive(Serialize)]
    enum Status {
        Active,
    }
    let val = to_value(&Status::Active).unwrap();
    assert_eq!(val, Value::Str("Active".into()));
}

#[test]
fn primitives() {
    assert_eq!(to_value(&true).unwrap(), Value::Bool(true));
    assert_eq!(to_value(&42_i64).unwrap(), Value::Int(42));
    assert_eq!(to_value(&2.5_f64).unwrap(), Value::Float(2.5));
    assert_eq!(to_value(&"hello").unwrap(), Value::Str("hello".into()));
}

#[test]
fn enum_struct_variant_auto_tags() {
    #[derive(Serialize)]
    enum Severity {
        Critical { reason: String },
        High,
    }

    // Struct variant → tagged dict (no #[serde(tag)] needed)
    let val = to_value(&Severity::Critical {
        reason: "urgent".into(),
    })
    .unwrap();
    let dict = match &val {
        Value::Struct(m) => m,
        other => panic!("expected Struct, got {}", other.type_name()),
    };
    assert_eq!(
        dict.get(crate::consts::ENUM_TAG_KEY),
        Some(&Value::Str("Critical".into()))
    );
    assert_eq!(dict.get("reason"), Some(&Value::Str("urgent".into())));

    // Unit variant → plain string (unchanged)
    let val = to_value(&Severity::High).unwrap();
    assert_eq!(val, Value::Str("High".into()));
}

// -- from_value tests --

#[test]
fn from_value_struct() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct Agent {
        name: String,
        score: i64,
    }
    let val = Value::new_struct([
        ("name", Value::Str("Alice".into())),
        ("score", Value::Int(95)),
    ]);
    let agent: Agent = from_value(&val).unwrap();
    assert_eq!(
        agent,
        Agent {
            name: "Alice".into(),
            score: 95
        }
    );
}

#[test]
fn from_value_primitives() {
    assert!(from_value::<bool>(&Value::Bool(true)).unwrap());
    assert_eq!(from_value::<i64>(&Value::Int(42)).unwrap(), 42);
    assert!((from_value::<f64>(&Value::Float(2.5)).unwrap() - 2.5).abs() < f64::EPSILON);
    assert_eq!(
        from_value::<String>(&Value::Str("hello".into())).unwrap(),
        "hello"
    );
}

#[test]
fn from_value_vec() {
    let val = Value::List(Arc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)]));
    let v: Vec<i64> = from_value(&val).unwrap();
    assert_eq!(v, vec![1, 2, 3]);
}

#[test]
fn from_value_enum_unit_variant() {
    #[derive(Deserialize, Debug, PartialEq)]
    enum Status {
        Active,
        Paused,
    }
    let val = Value::Str("Active".into());
    assert_eq!(from_value::<Status>(&val).unwrap(), Status::Active);
}

#[test]
fn from_value_enum_struct_variant() {
    #[derive(Deserialize, Debug, PartialEq)]
    enum Severity {
        Critical { reason: String },
        High,
    }
    let val = Value::new_struct([
        (crate::consts::ENUM_TAG_KEY, Value::Str("Critical".into())),
        ("reason", Value::Str("urgent".into())),
    ]);
    let sev: Severity = from_value(&val).unwrap();
    assert_eq!(
        sev,
        Severity::Critical {
            reason: "urgent".into()
        }
    );
}

#[test]
fn roundtrip_enum_struct_variant() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    enum Severity {
        Critical { reason: String },
        High,
    }

    let original = Severity::Critical {
        reason: "critical issue".into(),
    };
    let val = to_value(&original).unwrap();
    let restored: Severity = from_value(&val).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn roundtrip_enum_unit_variant() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    enum Severity {
        Critical { reason: String },
        High,
    }

    let original = Severity::High;
    let val = to_value(&original).unwrap();
    let restored: Severity = from_value(&val).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn roundtrip_struct() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Agent {
        name: String,
        score: i64,
        active: bool,
    }

    let original = Agent {
        name: "Bob".into(),
        score: 100,
        active: true,
    };
    let val = to_value(&original).unwrap();
    let restored: Agent = from_value(&val).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn from_value_type_mismatch_error() {
    let result = from_value::<i64>(&Value::Str("not a number".into()));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("expected int"),
        "error should mention expected type: {err}"
    );
}

#[test]
fn from_value_enum_missing_tag_error() {
    #[derive(Deserialize, Debug)]
    enum Status {
        Active,
    }
    // Struct without ENUM_TAG_KEY key
    let val = Value::new_struct([("name", Value::Str("oops".into()))]);
    let result = from_value::<Status>(&val);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains(crate::consts::ENUM_TAG_KEY),
        "error should mention missing tag key"
    );
}

// -- Value → Rust → Value roundtrips --

#[test]
fn value_rust_value_roundtrip_struct() {
    #[derive(Serialize, Deserialize)]
    struct Agent {
        name: String,
        score: i64,
    }

    let original = Value::new_struct([
        ("name", Value::Str("Alice".into())),
        ("score", Value::Int(95)),
    ]);
    let agent: Agent = from_value(&original).unwrap();
    let restored = to_value(&agent).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn value_rust_value_roundtrip_enum_struct() {
    #[derive(Serialize, Deserialize)]
    enum Severity {
        Critical { reason: String },
        High,
    }

    let original = Value::new_struct([
        (crate::consts::ENUM_TAG_KEY, Value::Str("Critical".into())),
        ("reason", Value::Str("urgent".into())),
    ]);
    let sev: Severity = from_value(&original).unwrap();
    let restored = to_value(&sev).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn value_rust_value_roundtrip_enum_unit() {
    #[derive(Serialize, Deserialize)]
    enum Severity {
        Critical { reason: String },
        High,
    }

    let original = Value::Str("High".into());
    let sev: Severity = from_value(&original).unwrap();
    let restored = to_value(&sev).unwrap();
    assert_eq!(original, restored);
}

// -- Convenience methods --

#[test]
fn value_from_serialize() {
    #[derive(Serialize)]
    struct Agent {
        name: String,
    }
    let val = Value::from_serialize(&Agent { name: "Bob".into() }).unwrap();
    assert_eq!(val.get_field("name"), Some(&Value::Str("Bob".into())));
}

#[test]
fn value_deserialize_into() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct Agent {
        name: String,
    }
    let val = Value::new_struct([("name", Value::Str("Bob".into()))]);
    let agent: Agent = val.deserialize_into().unwrap();
    assert_eq!(agent, Agent { name: "Bob".into() });
}

#[test]
fn value_deserialize_into_enum() {
    #[derive(Deserialize, Debug, PartialEq)]
    enum Status {
        Active,
        Paused,
    }
    let val = Value::Str("Paused".into());
    let status: Status = val.deserialize_into().unwrap();
    assert_eq!(status, Status::Paused);
}
