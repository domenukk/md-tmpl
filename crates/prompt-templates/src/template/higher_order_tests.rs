use std::sync::Arc;

use crate::{Template, Value};

#[test]
fn test_higher_order_template() {
    let helper = Template::from_source(
        "---\n\
         params: [name = str]\n\
         ---\n\
         Hello {{ name }}!",
    )
    .unwrap();

    let main = Template::from_source(
        "---\n\
         params: [test = tmpl<name = str>]\n\
         ---\n\
         > {% include test with name=\"World\" %}",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("test", Value::Tmpl(Arc::new(helper)));

    let result = main.render(&ctx).unwrap();
    assert_eq!(result, "Hello World!");
}

#[test]
fn test_higher_order_template_type_mismatch() {
    let helper = Template::from_source(
        "---\n\
         params: [age = int]\n\
         ---\n\
         Age: {{ age }}",
    )
    .unwrap();

    let main = Template::from_source(
        "---\n\
         params: [test = tmpl<name = str>]\n\
         ---\n\
         > {% include test with name=\"World\" %}",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("test", Value::Tmpl(Arc::new(helper)));

    let err = main.render(&ctx).unwrap_err();
    eprintln!("ACTUAL ERROR: {err}");
    assert!(
        err.to_string().contains("type mismatch")
            && err.to_string().contains("test")
            && err.to_string().contains("name")
            && err.to_string().contains("expected str"),
        "expected type mismatch error for 'test' at '.name', got: {err}"
    );
}

#[test]
fn test_higher_order_template_with_defaults() {
    let helper = Template::from_source(
        "---\n\
         params: [name = str, greeting = str := \"Hi\"]\n\
         ---\n\
         {{ greeting }} {{ name }}!",
    )
    .unwrap();

    let main = Template::from_source(
        "---\n\
         params: [test = tmpl<name = str>]\n\
         ---\n\
         > {% include test with name=\"World\" %}",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("test", Value::Tmpl(Arc::new(helper)));

    let result = main.render(&ctx).unwrap();
    assert_eq!(result, "Hi World!");
}

#[test]
fn test_higher_order_template_nested() {
    let inner = Template::from_source(
        "---\n\
         params: [val = str]\n\
         ---\n\
         Inner: {{ val }}",
    )
    .unwrap();

    let middle = Template::from_source(
        "---\n\
         params: [target = tmpl<val = str>, value = str]\n\
         ---\n\
         > {% include target with val=value %}",
    )
    .unwrap();

    let main = Template::from_source(
        "---\n\
         params: [processor = tmpl<target = tmpl<val = str>, value = str>, \
                  callback = tmpl<val = str>]\n\
         ---\n\
         > {% include processor with target=callback, value=\"Success\" %}",
    )
    .unwrap();

    let mut ctx = crate::Context::new();
    ctx.set("processor", Value::Tmpl(Arc::new(middle)));
    ctx.set("callback", Value::Tmpl(Arc::new(inner)));

    let result = main.render(&ctx).unwrap();
    assert_eq!(result, "Inner: Success");
}
