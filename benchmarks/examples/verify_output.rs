/// Quick script to print actual rendered output from each engine for visual inspection.
use prompt_templates::{Template, ctx, Value};
use std::sync::Arc;

fn main() {
    // ===== SIMPLE =====
    println!("═══ SIMPLE ═══");
    let pt = Template::from_source(r#"---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!"#).unwrap();
    let pt_out = pt.render_ctx(&ctx! { name: "Alice", place: "Wonderland" }).unwrap();
    println!("PT:   {pt_out:?}");

    let mut tera = tera::Tera::default();
    tera.add_raw_template("t", "Hello {{ name }}, welcome to {{ place }}!").unwrap();
    let json = serde_json::json!({"name": "Alice", "place": "Wonderland"});
    let tera_ctx = tera::Context::from_serialize(&json).unwrap();
    let tera_out = tera.render("t", &tera_ctx).unwrap();
    println!("TERA: {tera_out:?}");
    assert_eq!(pt_out.trim(), tera_out.trim(), "SIMPLE MISMATCH");

    // ===== HERO =====
    println!("\n═══ HERO ═══");
    let hero_pt = Template::from_source(r#"---
params:
  - title = str
  - sections = list(heading = str, entries = list(name = str, active = bool, score = float, tags = list(label = str)))
---
# {{ title }}

> {% for section in sections %}
## {{ section.heading }}

> {% for entry in section.entries %}
### {{ entry.name }}

> {% if entry.active %}

- Status: active
- Score: {{ entry.score | fixed(1) }}

> {% elif entry.score > 0 %}

- Status: inactive (score {{ entry.score | fixed(1) }})

> {% else %}

- Status: inactive

> {% /if %}
> {% for tag in entry.tags %}

  - tag: {{ tag.label }}

> {% /for %}
> {% /for %}
> {% /for %}"#).unwrap();

    fn mk_entry(name: &str, active: bool, score: f64, tags: &[&str]) -> Value {
        Value::new_struct([
            ("name", Value::from(name)),
            ("active", Value::from(active)),
            ("score", Value::Float(score)),
            ("tags", Value::List(Arc::new(
                tags.iter().map(|t| Value::new_struct([("label", Value::from(*t))])).collect(),
            ))),
        ])
    }
    let section_a = Value::new_struct([
        ("heading", Value::from("Overview")),
        ("entries", Value::List(Arc::new(vec![
            mk_entry("Service-A", true, 98.7, &["prod", "critical"]),
            mk_entry("Service-B", false, 45.2, &["staging"]),
            mk_entry("Service-C", false, 0.0, &["deprecated"]),
        ]))),
    ]);
    let section_b = Value::new_struct([
        ("heading", Value::from("Metrics")),
        ("entries", Value::List(Arc::new(vec![
            mk_entry("Latency", true, 12.3, &["p99"]),
            mk_entry("Throughput", false, 0.0, &["batch"]),
        ]))),
    ]);

    let hero_out = hero_pt.render_ctx(&ctx! {
        title: "System Report",
        sections: [(section_a), (section_b)],
    }).unwrap();
    println!("{hero_out}");
    println!("--- (length: {} bytes) ---", hero_out.len());

    println!("\n✅ All outputs verified!");
}
