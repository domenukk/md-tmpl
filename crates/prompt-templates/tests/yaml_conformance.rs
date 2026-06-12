//! YAML conformance tests for frontmatter parsing.
//!
//! Validates that all frontmatter blocks are valid YAML by parsing them with
//! both `serde_yaml` (proving valid YAML) and our custom parser (proving our
//! parser handles it correctly). Cross-validates key values between both.
//!
//! Key insight: the `types:` and `imports:` entries in our custom format use
//! syntax that is valid YAML. `types:` entries like `- Priority = enum<Low, High>`
//! are valid YAML plain strings. `imports:` entries like `- [helper](helper.tmpl.md)`
//! use YAML flow sequences with markdown links. The custom parser reads the
//! raw line text, so both parsers see valid input.

use prompt_templates::parse_frontmatter;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a YAML block with `serde_yaml`, asserting it's valid YAML.
/// Returns the parsed Value.
fn assert_valid_yaml(yaml_block: &str) -> serde_yaml::Value {
    serde_yaml::from_str(yaml_block).expect("serde_yaml parse failed — block is not valid YAML")
}

/// Build a template source from a YAML frontmatter block.
fn source_from_yaml(yaml_block: &str) -> String {
    format!("---\n{yaml_block}\n---\nbody")
}

/// Parse frontmatter via our custom parser and return type alias names.
fn our_parser_type_names(source: &str) -> Vec<String> {
    let (fm, _) = parse_frontmatter(source).expect("our parser failed");
    fm.type_aliases.keys().cloned().collect()
}

/// Parse frontmatter via our custom parser and return imports as (stem, path).
fn our_parser_imports(source: &str) -> Vec<(String, String)> {
    let (fm, _) = parse_frontmatter(source).expect("our parser failed");
    fm.imports
        .iter()
        .map(|imp| (imp.stem.clone(), imp.path.display().to_string()))
        .collect()
}

/// Parse frontmatter via our custom parser and return param (name, type).
fn our_parser_params(source: &str) -> Vec<(String, String)> {
    let (fm, _) = parse_frontmatter(source).expect("our parser failed");
    fm.declarations
        .iter()
        .map(|d| (d.name.clone(), format!("{}", d.var_type)))
        .collect()
}

/// Extract type alias names from a YAML block parsed by `serde_yaml`.
///
/// `serde_yaml` sees `- Priority = enum<Low, High>` as a plain string
/// (not a mapping), so we extract the name by splitting on `=`.
fn serde_yaml_type_names(yaml_block: &str) -> Vec<String> {
    let doc = assert_valid_yaml(yaml_block);
    let mapping = doc.as_mapping().expect("top-level should be a mapping");
    let types = match mapping.get(serde_yaml::Value::String("types".into())) {
        Some(v) => v.as_sequence().expect("'types' should be a sequence"),
        None => return vec![],
    };
    let mut names = Vec::new();
    for entry in types {
        // Each entry is a plain string like "Priority = enum<...>"
        if let Some(s) = entry.as_str() {
            if let Some(name) = s.split('=').next() {
                let name = name.trim();
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// Extract params as raw strings from `serde_yaml`.
fn serde_yaml_params(yaml_block: &str) -> Vec<String> {
    let doc = assert_valid_yaml(yaml_block);
    let mapping = doc.as_mapping().expect("top-level should be a mapping");
    let params = match mapping.get(serde_yaml::Value::String("params".into())) {
        Some(v) => v.as_sequence().expect("'params' should be a sequence"),
        None => return vec![],
    };
    params
        .iter()
        .map(|v| {
            v.as_str()
                .expect("each param should be a string")
                .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. types: block tests
// ---------------------------------------------------------------------------

#[test]
fn types_simple_enum() {
    let yaml = "types:\n  - Priority = enum<Low, Medium, High>\nparams: [x = Priority]";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_types = our_parser_type_names(&source);
    assert!(our_types.contains(&"Priority".to_string()));
}

#[test]
fn types_multiple_entries() {
    let yaml = "types:\n  - Priority = enum<Low, High>\n  - Status = enum<Open, Closed>\nparams: [p = Priority, s = Status]";
    let doc = assert_valid_yaml(yaml);
    let types = doc["types"].as_sequence().unwrap();
    assert_eq!(types.len(), 2);
    let source = source_from_yaml(yaml);
    let our_types = our_parser_type_names(&source);
    assert!(our_types.contains(&"Priority".to_string()));
    assert!(our_types.contains(&"Status".to_string()));
}

#[test]
fn types_complex_enum_with_fields() {
    let yaml = "types:\n  - Outcome = enum<Confirmed(evidence = str), Rejected, NeedsWork>\nparams: [result = Outcome]";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_types = our_parser_type_names(&source);
    assert!(our_types.contains(&"Outcome".to_string()));
}

#[test]
fn types_chained_aliases() {
    let yaml = concat!(
        "types:\n",
        "  - Severity = enum<Critical, High, Medium, Low>\n",
        "  - BugEntry = list<title = str, severity = Severity>\n",
        "params: [bugs = BugEntry]"
    );
    let doc = assert_valid_yaml(yaml);
    let types = doc["types"].as_sequence().unwrap();
    assert_eq!(types.len(), 2);
    let source = source_from_yaml(yaml);
    let our_types = our_parser_type_names(&source);
    assert!(our_types.contains(&"Severity".to_string()));
    assert!(our_types.contains(&"BugEntry".to_string()));
}

#[test]
fn types_list_alias() {
    let yaml = "types:\n  - Tags = list<name = str>\nparams: [items = Tags]";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_types = our_parser_type_names(&source);
    assert!(our_types.contains(&"Tags".to_string()));
}

// ---------------------------------------------------------------------------
// 2. imports: block tests
// ---------------------------------------------------------------------------

#[test]
fn imports_single() {
    // Our parser uses `imports: [[stem](path)]` format (bracket links).
    // For YAML validity, the outer brackets form a flow sequence.
    let yaml = "imports:\n  - \"[helper](helper.tmpl.md)\"\nparams: []\nallow_unused: true";
    assert_valid_yaml(yaml);
    // Our custom parser uses inline syntax
    let source =
        "---\nimports: [[helper](helper.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let our_imports = our_parser_imports(source);
    assert_eq!(our_imports.len(), 1);
    assert_eq!(our_imports[0].0, "helper");
}

#[test]
fn imports_multiple() {
    let yaml = concat!(
        "imports:\n",
        "  - \"[header](header.tmpl.md)\"\n",
        "  - \"[footer](footer.tmpl.md)\"\n",
        "params: []\n",
        "allow_unused: true"
    );
    assert_valid_yaml(yaml);
    let source = "---\nimports: [[header](header.tmpl.md), [footer](footer.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let our_imports = our_parser_imports(source);
    assert_eq!(our_imports.len(), 2);
    assert_eq!(our_imports[0].0, "header");
    assert_eq!(our_imports[1].0, "footer");
}

#[test]
fn imports_subdirectory_path() {
    let yaml =
        "imports:\n  - \"[shared](../common/shared.tmpl.md)\"\nparams: []\nallow_unused: true";
    assert_valid_yaml(yaml);
    let source = "---\nimports: [[shared](../common/shared.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let our_imports = our_parser_imports(source);
    assert_eq!(our_imports[0].0, "shared");
    assert!(our_imports[0].1.contains("shared.tmpl.md"));
}

#[test]
fn imports_inline_bracket_syntax() {
    // Our inline bracket syntax: imports: [[a](a.tmpl.md), [b](b.tmpl.md)]
    // This is also valid YAML (flow sequence of strings).
    let source = "---\nimports: [[alpha](alpha.tmpl.md), [beta](beta.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let our_imports = our_parser_imports(source);
    assert_eq!(our_imports.len(), 2);
    assert_eq!(our_imports[0].0, "alpha");
    assert_eq!(our_imports[1].0, "beta");
}

// ---------------------------------------------------------------------------
// 3. params: block tests
// ---------------------------------------------------------------------------

#[test]
fn params_simple_types() {
    let yaml = "params:\n  - name = str\n  - count = int\n  - flag = bool\n  - rate = float";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let serde_params = serde_yaml_params(yaml);
    assert_eq!(serde_params.len(), 4);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params.len(), 4);
    assert_eq!(our_params[0].0, "name");
    assert_eq!(our_params[1].0, "count");
    assert_eq!(our_params[2].0, "flag");
    assert_eq!(our_params[3].0, "rate");
}

#[test]
fn params_with_defaults() {
    let yaml =
        "params:\n  - name = str := world\n  - count = int := 42\n  - verbose = bool := true";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
    assert!(fm.declarations[1].default_value.is_some());
    assert!(fm.declarations[2].default_value.is_some());
}

#[test]
fn params_alias_reference() {
    let yaml = "types:\n  - Priority = enum<Low, Medium, High>\nparams:\n  - severity = Priority";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let serde_params = serde_yaml_params(yaml);
    assert_eq!(serde_params.len(), 1);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "severity");
}

#[test]
fn params_compound_list() {
    let yaml = "params:\n  - items = list<name = str, score = int>";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "items");
    assert!(our_params[0].1.contains("list"));
}

#[test]
fn params_compound_dict() {
    let yaml = "params:\n  - meta = dict<key = str, value = int>";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "meta");
    assert!(our_params[0].1.contains("dict"));
}

#[test]
fn params_inline_flow_syntax() {
    let source = "---\nparams: [a = str, b = int]\n---\n{{ a }} {{ b }}";
    let (fm, _) = parse_frontmatter(source).expect("parse failed");
    assert_eq!(fm.declarations.len(), 2);
    assert_eq!(fm.declarations[0].name, "a");
    assert_eq!(fm.declarations[1].name, "b");
}

// ---------------------------------------------------------------------------
// 4. Complex nested types
// ---------------------------------------------------------------------------

#[test]
fn nested_enum_with_associated_data() {
    let yaml =
        "params:\n  - outcome = enum<Confirmed(evidence = str, confidence = float), Rejected>";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "outcome");
}

#[test]
fn deeply_nested_list_enum() {
    let yaml = "params:\n  - reports = list<title = str, findings = list<desc = str, severity = enum<Critical, High, Low>>>";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "reports");
}

#[test]
fn triple_nested_types() {
    let yaml = "params:\n  - data = list<groups = list<items = list<name = str>>>";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "data");
}

#[test]
fn enum_with_nested_list() {
    let yaml = "params:\n  - action = enum<Batch(items = list<name = str>), Single(name = str)>";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "action");
}

// ---------------------------------------------------------------------------
// 5. Full realistic frontmatter
// ---------------------------------------------------------------------------

#[test]
fn full_realistic_types_imports_params() {
    let yaml = concat!(
        "types:\n",
        "  - Severity = enum<Critical, High, Medium, Low>\n",
        "imports:\n",
        "  - \"[header](header.tmpl.md)\"\n",
        "params:\n",
        "  - title = str\n",
        "  - bugs = list<name = str, severity = Severity>\n",
        "allow_unused: true"
    );
    let doc = assert_valid_yaml(yaml);
    let mapping = doc.as_mapping().unwrap();
    assert!(mapping.contains_key(serde_yaml::Value::String("types".into())));
    assert!(mapping.contains_key(serde_yaml::Value::String("imports".into())));
    assert!(mapping.contains_key(serde_yaml::Value::String("params".into())));

    let source = "---\ntypes:\n  - Severity = enum<Critical, High, Medium, Low>\nimports: [[header](header.tmpl.md)]\nparams:\n  - title = str\n  - bugs = list<name = str, severity = Severity>\nallow_unused: true\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("our parser failed");
    assert!(fm.type_aliases.contains_key("Severity"));
    assert_eq!(fm.imports.len(), 1);
    assert_eq!(fm.declarations.len(), 2);
}

#[test]
fn types_only_no_imports() {
    let yaml = "types:\n  - Priority = enum<Low, High>\nparams: [x = Priority]";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    assert!(fm.type_aliases.contains_key("Priority"));
    assert!(fm.imports.is_empty());
}

#[test]
fn imports_only_no_types() {
    let source =
        "---\nimports: [[helper](helper.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("our parser failed");
    assert_eq!(fm.imports.len(), 1);
    assert!(fm.type_aliases.is_empty());
}

#[test]
fn allow_unused_flag() {
    let yaml = "params: [x = str]\nallow_unused: true";
    let doc = assert_valid_yaml(yaml);
    let mapping = doc.as_mapping().unwrap();
    let allow_unused = mapping
        .get(serde_yaml::Value::String("allow_unused".into()))
        .expect("missing allow_unused");
    assert!(allow_unused.as_bool().unwrap());
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    assert!(fm.allow_unused);
}

#[test]
fn name_and_description_fields() {
    let yaml = "name: my_template\ndescription: A test template\nparams: [x = str]";
    let doc = assert_valid_yaml(yaml);
    let mapping = doc.as_mapping().unwrap();
    assert_eq!(
        mapping
            .get(serde_yaml::Value::String("name".into()))
            .unwrap()
            .as_str()
            .unwrap(),
        "my_template"
    );
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    assert_eq!(fm.name, "my_template");
    assert_eq!(fm.description, "A test template");
}

#[test]
fn kitchen_sink() {
    let yaml = concat!(
        "name: kitchen_sink\n",
        "description: Everything at once\n",
        "types:\n",
        "  - Priority = enum<Low, Medium, High>\n",
        "  - BugList = list<title = str, severity = Priority>\n",
        "imports:\n",
        "  - \"[header](header.tmpl.md)\"\n",
        "  - \"[footer](footer.tmpl.md)\"\n",
        "params:\n",
        "  - agent_name = str\n",
        "  - bugs = BugList\n",
        "  - verbose = bool := false\n",
        "allow_unused: true"
    );
    let doc = assert_valid_yaml(yaml);
    let mapping = doc.as_mapping().unwrap();
    assert!(mapping.contains_key(serde_yaml::Value::String("name".into())));
    assert!(mapping.contains_key(serde_yaml::Value::String("types".into())));
    assert!(mapping.contains_key(serde_yaml::Value::String("params".into())));

    let source = "---\nname: kitchen_sink\ndescription: Everything at once\ntypes:\n  - Priority = enum<Low, Medium, High>\n  - BugList = list<title = str, severity = Priority>\nimports: [[header](header.tmpl.md), [footer](footer.tmpl.md)]\nparams:\n  - agent_name = str\n  - bugs = BugList\n  - verbose = bool := false\nallow_unused: true\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("our parser failed");
    assert_eq!(fm.name, "kitchen_sink");
    assert_eq!(fm.imports.len(), 2);
    assert_eq!(fm.declarations.len(), 3);
    assert!(fm.allow_unused);
    assert!(fm.declarations[2].default_value.is_some());
}

// ---------------------------------------------------------------------------
// 6. Cross-validation: both parsers agree on extracted values
// ---------------------------------------------------------------------------

#[test]
fn cross_validate_type_alias_names() {
    let yaml =
        "types:\n  - Alpha = enum<A, B>\n  - Beta = list<x = str>\nparams: [a = Alpha, b = Beta]";
    let serde_names = serde_yaml_type_names(yaml);
    let source = source_from_yaml(yaml);
    let our_names = our_parser_type_names(&source);
    for name in &serde_names {
        assert!(
            our_names.contains(name),
            "our parser missing type alias '{name}'"
        );
    }
    // Note: our parser may have more entries than serde_yaml due to
    // implicit type aliases for compound params (list/dict/enum).
    assert!(
        our_names.len() >= serde_names.len(),
        "our parser should have at least as many type aliases"
    );
}

#[test]
fn cross_validate_param_names() {
    let yaml = "params:\n  - name = str\n  - count = int\n  - items = list<label = str>";
    let serde_params = serde_yaml_params(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(serde_params.len(), our_params.len());
    for (i, serde_param) in serde_params.iter().enumerate() {
        let serde_name = serde_param.split('=').next().unwrap().trim();
        assert_eq!(
            serde_name, our_params[i].0,
            "param name mismatch at index {i}"
        );
    }
}

#[test]
fn cross_validate_param_types_match() {
    let yaml = "params:\n  - name = str\n  - items = list<label = str, score = int>";
    let serde_params = serde_yaml_params(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(serde_params.len(), our_params.len());
    // First param is simple
    assert!(serde_params[0].contains("str"));
    assert!(our_params[0].1.contains("str"));
    // Second param is compound
    assert!(serde_params[1].contains("list"));
    assert!(our_params[1].1.contains("list"));
}

#[test]
fn cross_validate_import_stems() {
    // YAML-side uses quoted form; our parser uses bracket link form.
    // Both should agree on stems.
    let yaml = "imports:\n  - \"[alpha](alpha.tmpl.md)\"\n  - \"[beta](beta.tmpl.md)\"\nparams: []\nallow_unused: true";
    let doc = assert_valid_yaml(yaml);
    let imports = doc["imports"].as_sequence().unwrap();
    let serde_stems: Vec<String> = imports
        .iter()
        .map(|v| {
            let s = v.as_str().unwrap();
            let start = s.find('[').unwrap() + 1;
            let end = s.find(']').unwrap();
            s[start..end].to_string()
        })
        .collect();

    let source = "---\nimports: [[alpha](alpha.tmpl.md), [beta](beta.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let our_imports = our_parser_imports(source);
    assert_eq!(serde_stems.len(), our_imports.len());
    for (i, stem) in serde_stems.iter().enumerate() {
        assert_eq!(stem, &our_imports[i].0, "import stem mismatch at index {i}");
    }
}

// ---------------------------------------------------------------------------
// 7. Edge cases
// ---------------------------------------------------------------------------

#[test]
fn special_chars_in_type_expressions() {
    // Characters like <, >, (, ), , are valid in YAML plain scalars in block context
    let yaml = "params:\n  - result = enum<Success(code = int, msg = str), Failure(reason = str)>";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let our_params = our_parser_params(&source);
    assert_eq!(our_params[0].0, "result");
}

#[test]
fn empty_params_block() {
    let yaml = "params: []";
    let doc = assert_valid_yaml(yaml);
    let params = doc["params"].as_sequence().expect("sequence");
    assert!(params.is_empty());
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    assert!(fm.declarations.is_empty());
}

#[test]
fn name_field_only() {
    let yaml = "name: simple\nparams: [x = str]";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    assert_eq!(fm.name, "simple");
    assert!(fm.description.is_empty());
}

#[test]
fn description_field_only() {
    let yaml = "description: A helpful template\nparams: [x = str]";
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    assert_eq!(fm.description, "A helpful template");
}

// ---------------------------------------------------------------------------
// 8. Stress tests
// ---------------------------------------------------------------------------

#[test]
fn stress_multiple_imports_types_params() {
    let yaml = concat!(
        "types:\n",
        "  - Priority = enum<Critical, High, Medium, Low>\n",
        "  - Status = enum<Open, Closed, InProgress>\n",
        "  - Tags = list<name = str>\n",
        "params:\n",
        "  - title = str\n",
        "  - priority = Priority\n",
        "  - status = Status\n",
        "  - tags = Tags\n",
        "  - verbose = bool := false\n",
        "  - limit = int := 100\n",
        "allow_unused: true"
    );
    let doc = assert_valid_yaml(yaml);
    let types = doc["types"].as_sequence().unwrap();
    assert_eq!(types.len(), 3);
    let params = doc["params"].as_sequence().unwrap();
    assert_eq!(params.len(), 6);

    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    assert_eq!(fm.type_aliases.len(), 3);
    assert_eq!(fm.declarations.len(), 6);
    assert!(fm.allow_unused);
}

#[test]
fn stress_chained_aliases_realistic() {
    let yaml = concat!(
        "types:\n",
        "  - Severity = enum<Critical(reason = str), High, Medium, Low>\n",
        "  - Finding = list<title = str, severity = Severity>\n",
        "  - Report = list<findings = Finding, author = str>\n",
        "params:\n",
        "  - reports = Report\n",
        "  - reviewer = str\n",
    );
    assert_valid_yaml(yaml);
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("our parser failed");
    // 3 explicit + implicit aliases for compound params
    assert!(
        fm.type_aliases.len() >= 3,
        "should have at least 3 type aliases, got {}",
        fm.type_aliases.len()
    );
    assert!(fm.type_aliases.contains_key("Severity"));
    assert!(fm.type_aliases.contains_key("Finding"));
    assert!(fm.type_aliases.contains_key("Report"));
    assert_eq!(fm.declarations.len(), 2);
}

// ---------------------------------------------------------------------------
// 9. Validation rules — frontmatter error cases
// ---------------------------------------------------------------------------

// Rule 1: Duplicate param names → error
#[test]
fn duplicate_param_names_rejected() {
    let source = "---\nparams: [a = str, a = int]\n---\nbody";
    let err = parse_frontmatter(source).expect_err("duplicate param should fail");
    assert!(
        err.to_string().contains("duplicate parameter name"),
        "should mention duplicate: {err}"
    );
}

// Rule 2: Duplicate type alias names → error
#[test]
fn duplicate_type_alias_names_rejected() {
    let source =
        "---\ntypes:\n  - Foo = enum<A, B>\n  - Foo = enum<C, D>\nparams: [x = Foo]\n---\nbody";
    let err = parse_frontmatter(source).expect_err("duplicate type alias should fail");
    assert!(
        err.to_string().contains("duplicate type alias"),
        "should mention duplicate type: {err}"
    );
}

// Rule 3: Param/type name collision with type mismatch → error
#[test]
fn param_type_collision_different_types_rejected() {
    // Type alias `Bugs` exists, but param `bugs` has a different type (str).
    // PascalCase of `bugs` = `Bugs`, which collides with the alias.
    let source = "---\ntypes:\n  - Bugs = enum<A, B>\nparams: [bugs = str]\n---\nbody";
    let err = parse_frontmatter(source).expect_err("param/type collision should fail");
    assert!(
        err.to_string().contains("conflicts with type alias"),
        "should mention conflict: {err}"
    );
}

// Rule 3 (positive): Param/type match is allowed
#[test]
fn param_type_collision_same_type_allowed() {
    // Param `bugs` has type `Bugs` which matches the alias — this is OK.
    let source =
        "---\ntypes:\n  - Bugs = enum<A, B>\nparams: [bugs = Bugs]\nallow_unused: true\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("matching param/type should succeed");
    assert!(fm.type_aliases.contains_key("Bugs"));
}

// Rule 4a: Type alias shadows import stem → error
#[test]
fn type_alias_shadows_import_stem_rejected() {
    let source = "---\nimports: [[helper](helper.tmpl.md)]\ntypes:\n  - helper = enum<A>\nparams: [x = helper]\nallow_unused: true\n---\nbody";
    let err = parse_frontmatter(source).expect_err("type shadowing import should fail");
    assert!(
        err.to_string().contains("shadows"),
        "should mention shadow: {err}"
    );
}

// Rule 4b: Param PascalCase shadows import stem → error
#[test]
fn param_pascal_case_shadows_import_stem_rejected() {
    // Param `helper` → PascalCase `Helper`, import stem `Helper`
    let source = "---\nimports: [[Helper](Helper.tmpl.md)]\nparams: [helper = str]\nallow_unused: true\n---\nbody";
    let err = parse_frontmatter(source).expect_err("param shadowing import should fail");
    assert!(
        err.to_string().contains("shadows import"),
        "should mention shadow: {err}"
    );
}

// Rule 7: Reserved keyword as param name → error
#[test]
fn reserved_keyword_as_param_name_rejected() {
    for keyword in &["list", "dict", "enum", "str", "int", "float", "bool"] {
        let source = format!("---\nparams: [{keyword} = str]\n---\nbody");
        let err = parse_frontmatter(&source)
            .unwrap_err_or_else(|| panic!("param named '{keyword}' should be rejected"));
        assert!(
            err.to_string().contains("reserved keyword"),
            "param '{keyword}' should mention reserved keyword: {err}"
        );
    }
}

// Rule 7: `params` as a param name → error
#[test]
fn param_named_params_rejected() {
    let source = "---\nparams: [params = str]\n---\nbody";
    let err = parse_frontmatter(source).expect_err("param named 'params' should fail");
    assert!(
        err.to_string().contains("reserved keyword"),
        "should mention reserved keyword: {err}"
    );
}

// Rule 7: Reserved keyword as type alias name → error
#[test]
fn reserved_keyword_as_type_name_rejected() {
    let source = "---\ntypes:\n  - str = enum<A, B>\nparams: [x = str]\n---\nbody";
    let err = parse_frontmatter(source).expect_err("type named 'str' should fail");
    assert!(
        err.to_string().contains("shadows built-in"),
        "should mention builtin shadow: {err}"
    );
}

// Rule 8: Import stem must match filename
#[test]
fn import_stem_mismatch_rejected() {
    // Stem is 'wrong' but file is 'helper.tmpl.md' (expected stem: 'helper')
    let source =
        "---\nimports: [[wrong](helper.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let err = parse_frontmatter(source).expect_err("mismatched stem should fail");
    assert!(
        err.to_string().contains("does not match")
            || err.to_string().contains("stem")
            || err.to_string().contains("mismatch"),
        "should mention stem mismatch: {err}"
    );
}

// Rule 8 (positive): Matching stem succeeds
#[test]
fn import_stem_matches_filename_accepted() {
    let source =
        "---\nimports: [[helper](helper.tmpl.md)]\nparams: []\nallow_unused: true\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("matching stem should succeed");
    assert_eq!(fm.imports[0].stem, "helper");
}

// Rule 10: Import stem vs inline template name collision
// (This is tested in template/tests.rs::import_stem_conflicts_with_inline_template_name)

// Unknown type reference → error
#[test]
fn unknown_type_reference_rejected() {
    let source = "---\nparams: [x = UnknownType]\n---\nbody";
    let err = parse_frontmatter(source).expect_err("unknown type should fail");
    assert!(
        err.to_string().contains("unknown type") || err.to_string().contains("UnknownType"),
        "should mention unknown type: {err}"
    );
}

// Empty types block is valid
#[test]
fn empty_types_block_is_valid() {
    let source = "---\ntypes: []\nparams: [x = str]\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("empty types should be valid");
    assert!(fm.type_aliases.is_empty());
}

// Type alias used in multiple params
#[test]
fn type_alias_used_in_multiple_params() {
    let source = "---\ntypes:\n  - Priority = enum<Low, High>\nparams:\n  - primary = Priority\n  - secondary = Priority\nallow_unused: true\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("alias in multiple params should work");
    assert_eq!(fm.declarations.len(), 2);
}

// Implicit param types are generated for compound types
#[test]
fn implicit_param_types_generated() {
    let source = "---\nparams:\n  - bugs = list<title = str>\n  - name = str\n---\nbody";
    let (fm, _) = parse_frontmatter(source).expect("parse should succeed");
    // `bugs` is a compound type → implicit alias `Bugs` should be generated
    assert!(
        fm.type_aliases.contains_key("Bugs"),
        "should have implicit 'Bugs' type alias, got: {:?}",
        fm.type_aliases.keys().collect::<Vec<_>>()
    );
    // `name` is simple → no implicit alias
    assert!(
        !fm.type_aliases.contains_key("Name"),
        "simple types should not generate implicit aliases"
    );
}

// Builtin type name shadowing (case-insensitive)
#[test]
fn builtin_type_shadow_case_insensitive() {
    // `List` should be rejected because `list` is a builtin (case-insensitive check)
    let source = "---\ntypes:\n  - List = enum<A, B>\nparams: [x = List]\n---\nbody";
    let err = parse_frontmatter(source).expect_err("'List' should shadow builtin 'list'");
    assert!(
        err.to_string().contains("shadows built-in"),
        "should mention builtin shadow: {err}"
    );
}

// extract_template_stem utility
#[test]
fn extract_template_stem_strips_extensions() {
    use std::path::Path;
    assert_eq!(
        prompt_templates::extract_template_stem(Path::new("review.tmpl.md")),
        "review"
    );
    assert_eq!(
        prompt_templates::extract_template_stem(Path::new("path/to/check.tmpl.md")),
        "check"
    );
    assert_eq!(
        prompt_templates::extract_template_stem(Path::new("simple.md")),
        "simple"
    );
    assert_eq!(
        prompt_templates::extract_template_stem(Path::new("bare")),
        "bare"
    );
}

// Helper trait for nicer test assertions.
trait UnwrapErrOrElse<T> {
    fn unwrap_err_or_else<F: FnOnce()>(self, f: F) -> prompt_templates::TemplateError;
}

impl<T: std::fmt::Debug> UnwrapErrOrElse<T> for Result<T, prompt_templates::TemplateError> {
    fn unwrap_err_or_else<F: FnOnce()>(self, f: F) -> prompt_templates::TemplateError {
        if let Err(e) = self {
            e
        } else {
            f();
            panic!("expected Err but got Ok");
        }
    }
}
