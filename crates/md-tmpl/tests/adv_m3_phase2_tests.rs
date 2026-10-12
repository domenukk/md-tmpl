//! Tier 5 White-box Adversarial Coverage Tests for Milestone M3 Phase 2.
//!
//! Evaluates buffer-reusing rendering parity, flexbuffer decoding boundaries,
//! and resource limits / filter boundary conditions.

use md_tmpl::{Context, Template, ctx};

#[test]
fn test_adv_rendering_into_buffer_with_inline_templates() {
    let src = "\
---
params:
  - name = str
---
> {% tmpl greeting %}
---
params:
  - name = str
---
Hello {{ name }}!

> {% /tmpl %}

> {% include greeting with name=name %}";

    let tmpl = Template::from_source(src).expect("compile failed");
    let ctx = ctx! { name: "Alice" };

    // Standard render
    let checked = tmpl.render_ctx(&ctx).expect("checked render failed");
    assert_eq!(checked, "Hello Alice!\n");

    // Render into reusable buffer
    let mut buf = String::new();
    tmpl.render_ctx_into(&ctx, &mut buf)
        .expect("render into failed");
    assert_eq!(buf, "Hello Alice!\n");
}

#[test]
fn test_adv_allowing_extra_rendering_ignores_undeclared_params() {
    let src = "\
---
params:
  - x = int
---
Result: {{ x }}";
    let tmpl = Template::from_source(src).expect("compile failed");
    let ctx = ctx! { x: 42, extra_param: "should be ignored" };

    let output = tmpl
        .render_ctx_allowing_extra(&ctx)
        .expect("allowing_extra render failed");
    assert_eq!(output, "Result: 42");
}

#[cfg(feature = "flexbuffers")]
#[test]
fn test_adv_flexbuffers_malformed_input_handling() {
    // Empty buffer
    let res = Context::from_flexbuffers(&[]);
    assert!(res.is_err(), "empty flexbuffer should return error");

    // Random garbage bytes
    let garbage = [0xde, 0xad, 0xbe, 0xef, 0x00, 0xff];
    let res = Context::from_flexbuffers(&garbage);
    assert!(
        res.is_err(),
        "garbage flexbuffer should return error without panicking"
    );
}

#[test]
fn test_adv_filter_fixed_precision_boundaries() {
    let src = "\
---
params:
  - val = float
---
{{ val | fixed(0) }} | {{ val | fixed(5) }}";
    let tmpl = Template::from_source(src).expect("compile failed");
    let pi = std::f64::consts::PI;
    let output = tmpl.render_ctx(&ctx! { val: pi }).expect("render failed");
    assert_eq!(output, "3 | 3.14159");
}

#[test]
fn test_adv_filter_limit_boundaries() {
    let src = "\
---
params:
  - items = list(int)
---
{{ items | limit(2) | join(\",\") }}";
    let tmpl = Template::from_source(src).expect("compile failed");

    // More items than limit
    let output = tmpl
        .render_ctx(&ctx! { items: [ 10, 20, 30, 40 ] })
        .expect("render failed");
    assert_eq!(output, "10,20");

    // Fewer items than limit
    let output = tmpl
        .render_ctx(&ctx! { items: [ 99 ] })
        .expect("render failed");
    assert_eq!(output, "99");

    // Zero limit
    let src_zero = "\
---
params:
  - items = list(int)
---
{{ items | limit(0) | join(\",\") }}";
    let tmpl_zero = Template::from_source(src_zero).expect("compile failed");
    let output = tmpl_zero
        .render_ctx(&ctx! { items: [ 10, 20 ] })
        .expect("render failed");
    assert_eq!(output, "");
}
