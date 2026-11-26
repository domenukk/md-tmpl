//! Strongly typed builder for generating Python class source code.
//!
//! Instead of raw string concatenation, this module provides Rust types
//! that represent Python class structure. The builder validates invariants
//! at construction time and renders correct Python source.
//!
//! ```text
//! PyClassDef ──┬── slots: [Field]
//!              ├── match_args: Option<[Field]>
//!              ├── class_attrs: [(name, value)]
//!              ├── methods: [PyMethodDef]
//!              ├── properties: [PyPropertyDef]
//!              └── inner_classes: [PyClassDef]
//! ```

use std::fmt::Write;

use pyo3::{Py, prelude::*, types::PyDict};
use typed_builder::TypedBuilder;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A field with a name and Python type annotation.
#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub name: String,
    pub wire_name: String,
    pub annotation: String,
}

impl Field {
    pub fn new(name: impl Into<String>, annotation: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            wire_name: name.clone(),
            name,
            annotation: annotation.into(),
        }
    }

    pub fn with_wire_name(
        name: impl Into<String>,
        wire_name: impl Into<String>,
        annotation: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            wire_name: wire_name.into(),
            annotation: annotation.into(),
        }
    }
}

/// A Python method definition.
#[derive(Debug, Clone, TypedBuilder)]
pub(crate) struct PyMethodDef {
    #[builder(setter(into))]
    pub name: String,
    #[builder(default)]
    pub params: Vec<Field>,
    #[builder(default, setter(strip_option, into))]
    pub return_annotation: Option<String>,
    #[builder(default, setter(strip_option, into))]
    pub doc: Option<String>,
    /// Lines of the method body (without leading indent).
    pub body: Vec<String>,
}

/// A Python property definition.
#[derive(Debug, Clone)]
pub(crate) struct PyPropertyDef {
    pub name: String,
    pub return_annotation: Option<String>,
    /// Lines of the property body (without leading indent).
    pub body: Vec<String>,
}

/// A class-level attribute with a name and literal Python expression.
#[derive(Debug, Clone)]
pub(crate) struct ClassAttr {
    pub name: String,
    pub value: String,
}

impl ClassAttr {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// PyClassDef
// ---------------------------------------------------------------------------

/// A strongly typed Python class definition.
///
/// Use [`PyClassDef::build`] to create one, configure it with builder
/// methods, then call [`PyClassDef::render`] to produce Python source
/// or [`PyClassDef::exec`] to execute it and extract the class object.
#[derive(Debug, Clone)]
pub(crate) struct PyClassDef {
    pub name: String,
    pub doc: Option<String>,
    pub slots: Vec<Field>,
    pub match_args: Option<Vec<Field>>,
    pub class_attrs: Vec<ClassAttr>,
    pub methods: Vec<PyMethodDef>,
    pub properties: Vec<PyPropertyDef>,
    pub inner_classes: Vec<PyClassDef>,
    /// Indent level (number of 4-space indents). Top-level = 0.
    indent: usize,
}

impl PyClassDef {
    /// Create a new class definition with the given name.
    pub fn build(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            doc: None,
            slots: Vec::new(),
            match_args: None,
            class_attrs: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            inner_classes: Vec::new(),
            indent: 0,
        }
    }

    /// Set the docstring.
    pub fn doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    /// Set `__slots__`.
    pub fn slots(mut self, slots: Vec<Field>) -> Self {
        self.slots = slots;
        self
    }

    /// Set `__match_args__` for structural pattern matching.
    pub fn match_args(mut self, args: Vec<Field>) -> Self {
        self.match_args = Some(args);
        self
    }

    /// Add a class-level attribute.
    pub fn attr(mut self, attr: ClassAttr) -> Self {
        self.class_attrs.push(attr);
        self
    }

    /// Add a method.
    pub fn method(mut self, method: PyMethodDef) -> Self {
        self.methods.push(method);
        self
    }

    /// Add a property.
    pub fn property(mut self, prop: PyPropertyDef) -> Self {
        self.properties.push(prop);
        self
    }

    /// Add an inner (nested) class.
    pub fn inner_class(mut self, mut cls: PyClassDef) -> Self {
        cls.indent = self.indent + 1;
        self.inner_classes.push(cls);
        self
    }

    /// Render this class definition as a Python source string.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(2048);
        self.render_into(&mut out);
        out
    }

    /// Execute the rendered source and extract the class object.
    pub fn exec(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_with_locals(py, None)
    }

    /// Execute the rendered source with extra bindings available in scope.
    ///
    /// Extra bindings are placed in the **globals** dict because Python class
    /// bodies resolve names from globals, not locals.
    pub fn exec_with_locals(
        &self,
        py: Python<'_>,
        extra_globals: Option<&[(String, Py<PyAny>)]>,
    ) -> PyResult<Py<PyAny>> {
        let source = self.render();
        let globals = PyDict::new(py);
        // Add builtins so the generated code can use built-in names.
        let builtins = py.import("builtins")?;
        globals.set_item("__builtins__", builtins)?;
        if let Some(extras) = extra_globals {
            for (name, obj) in extras {
                globals.set_item(name, obj)?;
            }
        }
        let locals = PyDict::new(py);
        py.run(
            &std::ffi::CString::new(source.as_str()).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "internal error building class '{}': {e}",
                    self.name
                ))
            })?,
            Some(&globals),
            Some(&locals),
        )?;

        locals
            .get_item(&self.name)?
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "failed to generate class '{}'",
                    self.name
                ))
            })
            .map(pyo3::Bound::unbind)
    }

    fn render_into(&self, out: &mut String) {
        let base = "    ".repeat(self.indent);
        let member = "    ".repeat(self.indent + 1);
        let body = "    ".repeat(self.indent + 2);

        // Class header.
        writeln!(out, "{base}class {}:", self.name).expect("write to String");

        // Docstring.
        if let Some(doc) = &self.doc {
            write!(out, "{member}\"\"\"{}\"\"\"\n\n", py_docstring_escape(doc))
                .expect("write to String");
        }

        // __match_args__
        if let Some(args) = &self.match_args {
            let names: Vec<String> = args
                .iter()
                .map(|f| py_single_quoted_literal(&f.name))
                .collect();
            writeln!(
                out,
                "{member}__match_args__ = ({})",
                py_tuple_literal(&names)
            )
            .expect("write to String");
        }

        // __slots__
        if !self.slots.is_empty() {
            let names: Vec<String> = self
                .slots
                .iter()
                .map(|f| py_single_quoted_literal(&f.name))
                .collect();
            writeln!(out, "{member}__slots__ = ({})", py_tuple_literal(&names))
                .expect("write to String");
        }

        // Inner classes (before class attributes, since attrs may reference inner classes).
        for inner in &self.inner_classes {
            inner.render_into(out);
        }

        // Class attributes (may reference inner classes defined above).
        for attr in &self.class_attrs {
            writeln!(out, "{member}{} = {}", attr.name, attr.value).expect("write to String");
        }

        // Add spacing after class-level declarations.
        if self.match_args.is_some()
            || !self.slots.is_empty()
            || !self.class_attrs.is_empty()
            || !self.inner_classes.is_empty()
        {
            out.push('\n');
        }

        // Methods.
        for method in &self.methods {
            render_method(out, method, &member, &body);
        }

        // Properties.
        for prop in &self.properties {
            render_property(out, prop, &member, &body);
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn render_method(out: &mut String, method: &PyMethodDef, member: &str, body: &str) {
    // Signature — build directly into `out` to avoid intermediate allocations.
    write!(out, "{member}def {}(self", method.name).expect("write to String");
    for p in &method.params {
        out.push_str(", ");
        if p.annotation.is_empty() {
            out.push_str(&p.name);
        } else {
            write!(out, "{}: {}", p.name, p.annotation).expect("write to String");
        }
    }
    out.push(')');

    if let Some(ret) = &method.return_annotation {
        write!(out, " -> {ret}").expect("write to String");
    }
    out.push_str(":\n");

    // Docstring.
    if let Some(doc) = &method.doc {
        writeln!(out, "{body}\"\"\"{}\"\"\"", py_docstring_escape(doc)).expect("write to String");
    }

    // Body.
    for line in &method.body {
        if line.is_empty() {
            out.push('\n');
        } else {
            writeln!(out, "{body}{line}").expect("write to String");
        }
    }
    out.push('\n');
}

fn render_property(out: &mut String, prop: &PyPropertyDef, member: &str, body: &str) {
    writeln!(out, "{member}@property").expect("write to String");
    write!(out, "{member}def {}(self)", prop.name).expect("write to String");
    if let Some(ret) = &prop.return_annotation {
        write!(out, " -> {ret}").expect("write to String");
    }
    out.push_str(":\n");
    for line in &prop.body {
        if line.is_empty() {
            out.push('\n');
        } else {
            writeln!(out, "{body}{line}").expect("write to String");
        }
    }
    out.push('\n');
}

/// Format items as a Python tuple literal, with a trailing comma for
/// single-element tuples.
fn py_tuple_literal(items: &[String]) -> String {
    if items.len() == 1 {
        format!("{},", items[0])
    } else {
        items.join(", ")
    }
}

/// Escape an arbitrary string as a single-quoted Python string literal (`'...'`).
pub(crate) fn py_single_quoted_literal(s: &str) -> String {
    py_quoted_literal(s, '\'')
}

/// Escape an arbitrary string as a double-quoted Python string literal (`"..."`).
pub(crate) fn py_double_quoted_literal(s: &str) -> String {
    py_quoted_literal(s, '"')
}

fn py_quoted_literal(s: &str, quote: char) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\x7f' => {
                write!(out, "\\x{:02x}", c as u32).expect("write to String");
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Escape a docstring body so it cannot break out of a `"""..."""` literal.
pub(crate) fn py_docstring_escape(doc: &str) -> String {
    let mut escaped = doc
        .replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace("\"\"\"", "\\\"\\\"\\\"");
    if escaped.ends_with('"') {
        let trailing_backslashes = escaped[..escaped.len() - 1]
            .bytes()
            .rev()
            .take_while(|&b| b == b'\\')
            .count();
        if trailing_backslashes % 2 == 0 {
            escaped.pop();
            escaped.push_str("\\\"");
        }
    }
    escaped
}

// ---------------------------------------------------------------------------
// Convenience constructors for common dunder methods
// ---------------------------------------------------------------------------

impl PyClassDef {
    /// Add a standard `__init__` that assigns fields to self.
    pub fn with_init(self, fields: &[Field]) -> Self {
        let body: Vec<String> = fields
            .iter()
            .map(|f| format!("self.{n} = {n}", n = f.name))
            .collect();
        self.method(
            PyMethodDef::builder()
                .name("__init__")
                .params(fields.to_vec())
                .body(body)
                .build(),
        )
    }

    /// Add a `__repr__` returning `ClassName(field=val, ...)`.
    pub fn with_repr(self, class_name: &str, fields: &[Field]) -> Self {
        let parts: Vec<String> = fields
            .iter()
            .map(|f| format!("{}={{self.{}!r}}", f.name, f.name))
            .collect();
        self.method(
            PyMethodDef::builder()
                .name("__repr__")
                .return_annotation("str")
                .body(vec![format!(
                    "return f'{class_name}({})'",
                    parts.join(", ")
                )])
                .build(),
        )
    }

    /// Add a `__eq__` comparing all fields.
    pub fn with_eq(self, fields: &[Field]) -> Self {
        let mut body = vec!["if not isinstance(other, type(self)):".into()];
        body.push("    return NotImplemented".into());
        if fields.is_empty() {
            body.push("return True".into());
        } else {
            let parts: Vec<String> = fields
                .iter()
                .map(|f| format!("self.{} == other.{}", f.name, f.name))
                .collect();
            body.push(format!("return {}", parts.join(" and ")));
        }
        self.method(
            PyMethodDef::builder()
                .name("__eq__")
                .params(vec![Field::new("other", "")])
                .return_annotation("bool")
                .body(body)
                .build(),
        )
    }

    /// Add a `__hash__` hashing a tag and all fields.
    pub fn with_hash(self, tag: &str, fields: &[Field]) -> Self {
        let parts: Vec<String> = fields.iter().map(|f| format!("self.{}", f.name)).collect();
        let quoted_tag = py_single_quoted_literal(tag);
        let hash_expr = if parts.is_empty() {
            format!("return hash({quoted_tag})")
        } else {
            format!("return hash(({quoted_tag}, {}))", parts.join(", "))
        };
        self.method(
            PyMethodDef::builder()
                .name("__hash__")
                .return_annotation("int")
                .body(vec![hash_expr])
                .build(),
        )
    }

    /// Add a `_md_tmpl_fields` property returning a dict of fields.
    pub fn with_fields_property(self, fields: &[Field]) -> Self {
        let items: Vec<String> = fields
            .iter()
            .map(|f| {
                format!(
                    "{}: self.{}",
                    py_single_quoted_literal(&f.wire_name),
                    f.name
                )
            })
            .collect();
        self.property(PyPropertyDef {
            name: "_md_tmpl_fields".into(),
            return_annotation: Some("dict".into()),
            body: vec![format!("return {{{}}}", items.join(", "))],
        })
    }

    /// Add a `__dict__` property returning a dict of all fields.
    pub fn with_dict_property(self, fields: &[Field]) -> Self {
        let items: Vec<String> = fields
            .iter()
            .map(|f| {
                format!(
                    "{}: self.{}",
                    py_single_quoted_literal(&f.wire_name),
                    f.name
                )
            })
            .collect();
        self.property(PyPropertyDef {
            name: "__dict__".into(),
            return_annotation: None,
            body: vec![format!("return {{{}}}", items.join(", "))],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docstring_escape_handles_trailing_triple_and_escaped_quotes() {
        assert_eq!(py_docstring_escape("foo\"\"\""), "foo\\\"\\\"\\\"");
        assert_eq!(py_docstring_escape("foo\""), "foo\\\"");
        assert_eq!(py_docstring_escape("foo\\\""), "foo\\\\\\\"");
        assert_eq!(
            py_docstring_escape("\"\"\"\"\"\""),
            "\\\"\\\"\\\"\\\"\\\"\\\""
        );
    }

    #[test]
    fn simple_class_renders() {
        let fields = vec![Field::new("name", "str"), Field::new("age", "int")];
        let cls = PyClassDef::build("Person")
            .doc("A person.")
            .slots(fields.clone())
            .with_init(&fields)
            .with_repr("Person", &fields)
            .with_eq(&fields);

        let source = cls.render();
        assert!(source.contains("class Person:"));
        assert!(source.contains("\"\"\"A person.\"\"\""));
        assert!(source.contains("__slots__"));
        assert!(source.contains("def __init__(self, name: str, age: int):"));
        assert!(source.contains("self.name = name"));
        assert!(source.contains("def __repr__(self) -> str:"));
        assert!(source.contains("def __eq__(self, other) -> bool:"));
    }

    #[test]
    fn single_field_tuple_has_trailing_comma() {
        let fields = vec![Field::new("x", "int")];
        let cls = PyClassDef::build("Single").slots(fields);
        let source = cls.render();
        assert!(
            source.contains("__slots__ = ('x',)"),
            "single-element tuple needs trailing comma, got: {source}"
        );
    }

    #[test]
    fn nested_class_renders() {
        let inner = PyClassDef::build("Inner")
            .doc("Nested.")
            .slots(vec![Field::new("val", "int")]);
        let outer = PyClassDef::build("Outer")
            .doc("Container.")
            .inner_class(inner);

        let source = outer.render();
        assert!(source.contains("class Outer:"));
        assert!(source.contains("    class Inner:"));
        assert!(source.contains("        \"\"\"Nested.\"\"\""));
    }

    #[test]
    fn match_args_renders() {
        let fields = vec![Field::new("reason", "str")];
        let cls = PyClassDef::build("NeedsChanges")
            .match_args(fields.clone())
            .slots(fields);
        let source = cls.render();
        assert!(
            source.contains("__match_args__ = ('reason',)"),
            "got: {source}"
        );
    }

    #[test]
    fn class_attr_renders() {
        let cls = PyClassDef::build("Variant").attr(ClassAttr::new("_md_tmpl_tag", "'Confirmed'"));
        let source = cls.render();
        assert!(source.contains("_md_tmpl_tag = 'Confirmed'"));
    }

    #[test]
    fn property_renders() {
        let cls = PyClassDef::build("MyClass").property(PyPropertyDef {
            name: "value".into(),
            return_annotation: Some("int".into()),
            body: vec!["return 42".into()],
        });
        let source = cls.render();
        assert!(source.contains("@property"));
        assert!(source.contains("def value(self) -> int:"));
        assert!(source.contains("return 42"));
    }

    #[test]
    fn windows_path_escaping_in_python_literals_and_docstrings() {
        assert_eq!(
            py_double_quoted_literal("crates/md-tmpl/prompts/greeting.tmpl.md"),
            "\"crates/md-tmpl/prompts/greeting.tmpl.md\""
        );
        assert_eq!(
            py_single_quoted_literal(r"C:\Users\alice\prompts\task_report.tmpl.md"),
            r"'C:\\Users\\alice\\prompts\\task_report.tmpl.md'"
        );
        assert_eq!(
            py_single_quoted_literal(r"C:\Users\o'neil\file.tmpl.md"),
            r"'C:\\Users\\o\'neil\\file.tmpl.md'"
        );
        assert_eq!(
            py_docstring_escape(r"Typed for C:\Users\alice\greeting.tmpl.md"),
            r"Typed for C:\\Users\\alice\\greeting.tmpl.md"
        );
        assert_eq!(
            py_docstring_escape("crates/md-tmpl/prompts/greeting.tmpl.md"),
            "crates/md-tmpl/prompts/greeting.tmpl.md"
        );

        let cls = PyClassDef::build("WinTest")
            .doc(r"Template 'C:\Users\alice\t.tmpl.md'")
            .attr(ClassAttr::new(
                "_template_path",
                py_single_quoted_literal(r"C:\Users\alice\t.tmpl.md"),
            ));
        let rendered = cls.render();
        assert!(rendered.contains(r"C:\\Users\\alice\\t.tmpl.md"));
    }
}
