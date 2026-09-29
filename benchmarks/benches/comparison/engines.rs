//! Engine wrappers and correctness assertion helpers for comparison benchmarks.

use handlebars::Handlebars;
use md_tmpl::Template;
use minijinja::Environment;

pub const TEMPLATE_NAME: &str = "bench";

// ==========================================================================
// Engine wrappers — pre-compiled template holders
// ==========================================================================

pub struct MdTmplEngine {
    pub template: Template,
}

impl MdTmplEngine {
    pub fn compile(source: &str) -> Self {
        Self {
            template: Template::from_source(source)
                .expect("md-tmpl: failed to compile template"),
        }
    }

    /// Render with context validation and unknown parameter rejection.
    #[inline]
    pub fn render_ctx(&self, ctx: &md_tmpl::Context) -> String {
        self.template
            .render_ctx(ctx)
            .expect("md-tmpl: render_ctx failed")
    }
}

pub struct TeraEngine {
    pub engine: tera::Tera,
}

impl TeraEngine {
    pub fn compile(source: &str) -> Self {
        let mut engine = tera::Tera::default();
        engine
            .add_raw_template(TEMPLATE_NAME, source)
            .expect("tera: failed to compile template");
        Self { engine }
    }

    /// Build a reusable Tera context from any Serialize type.
    pub fn context(data: &impl serde::Serialize) -> tera::Context {
        tera::Context::from_serialize(data).expect("tera: failed to serialize context")
    }

    /// Render with a pre-built context (used in benchmark loops).
    pub fn render_ctx(&self, ctx: &tera::Context) -> String {
        self.engine
            .render(TEMPLATE_NAME, ctx)
            .expect("tera: render failed")
    }
}

pub struct MiniJinjaEngine {
    pub env: Environment<'static>,
}

impl MiniJinjaEngine {
    pub fn compile(source: &'static str) -> Self {
        let mut env = Environment::new();
        env.add_template_owned(TEMPLATE_NAME.to_owned(), source.to_owned())
            .expect("minijinja: failed to compile template");
        Self { env }
    }

    /// Render directly from any Serialize type — MiniJinja's optimal path.
    pub fn render(&self, data: &impl serde::Serialize) -> String {
        let tmpl = self
            .env
            .get_template(TEMPLATE_NAME)
            .expect("minijinja: template not found");
        tmpl.render(data)
            .expect("minijinja: render failed")
    }
}

pub struct HandlebarsEngine {
    pub registry: Handlebars<'static>,
}

impl HandlebarsEngine {
    pub fn compile(source: &str) -> Self {
        let mut registry = Handlebars::new();
        registry.set_strict_mode(true);
        // Disable HTML escaping — we produce plain text.
        registry.register_escape_fn(handlebars::no_escape);
        registry
            .register_template_string(TEMPLATE_NAME, source)
            .expect("handlebars: failed to compile template");
        Self { registry }
    }

    pub fn render(&self, data: &serde_json::Value) -> String {
        self.registry
            .render(TEMPLATE_NAME, data)
            .expect("handlebars: render failed")
    }
}

// ==========================================================================
// Correctness assertions
// ==========================================================================

pub fn normalize(s: &str) -> String {
    s.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Assert that all engines produce the same output for a scenario.
/// `expected` is optional — if `Some`, also verify against the known value.
pub fn assert_engines_match(
    scenario: &str,
    pt_output: &str,
    tera_output: &str,
    mj_output: &str,
    hbs_output: &str,
    expected: Option<&str>,
) {
    let pt = normalize(pt_output);
    let tera = normalize(tera_output);
    let mj = normalize(mj_output);
    let hbs = normalize(hbs_output);

    assert_eq!(
        pt, tera,
        "[{scenario}] md-tmpl vs tera mismatch:\nPT:\n{pt}\n\nTERA:\n{tera}"
    );
    assert_eq!(
        pt, mj,
        "[{scenario}] md-tmpl vs minijinja mismatch:\nPT:\n{pt}\n\nMJ:\n{mj}"
    );
    assert_eq!(
        pt, hbs,
        "[{scenario}] md-tmpl vs handlebars mismatch:\nPT:\n{pt}\n\nHBS:\n{hbs}"
    );

    if let Some(exp) = expected {
        let exp_norm = normalize(exp);
        assert_eq!(
            pt, exp_norm,
            "[{scenario}] output does not match expected:\nGOT:\n{pt}\n\nEXPECTED:\n{exp_norm}"
        );
    }
}
