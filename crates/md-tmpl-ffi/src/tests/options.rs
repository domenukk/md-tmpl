//! Compile options: base dir, frontmatter, env, allow-unused, files, and cached includes.

use std::ptr;

use crate::*;

#[test]
fn test_from_source_with_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("header.tmpl.md"),
        "\
---
name: header
params: [title = str]
---
# {{ title }}",
    )
    .unwrap();

    let source = CString::new(
        "\
---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body",
    )
    .unwrap();
    let base_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_base_dir(source.as_ptr(), base_dir.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null(), "error: {:?}", unsafe {
        err.as_ref().map(|p| CStr::from_ptr(p))
    });
    assert!(!tmpl.is_null());

    let ctx = pt_context_new();
    let key = CString::new("title").unwrap();
    let val = CString::new("Test").unwrap();
    unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert!(
        result_str.contains("Test"),
        "expected Test in: {result_str}"
    );

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_frontmatter() {
    let source = CString::new(
        "\
---
name: greeting
description: A greeting template
params: [name = str]
---
Hello {{ name }}!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let mut fm: *mut c_char = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
    };
    assert!(err.is_null(), "expected no error");
    assert!(!tmpl.is_null());
    assert!(!fm.is_null());

    let fm_str = unsafe { CStr::from_ptr(fm) }.to_str().unwrap();
    assert!(
        fm_str.contains("\"name\":\"greeting\""),
        "expected name in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"description\":\"A greeting template\""),
        "expected desc in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"has_params\":true"),
        "expected has_params in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"allow_unused\":false"),
        "expected allow_unused in fm: {fm_str}"
    );

    // Verify template still works
    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("World").unwrap();
    unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "Hello World!");

    unsafe {
        pt_free_string(fm);
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_frontmatter_no_params() {
    let source = CString::new(
        "\
---
name: static
description: No params
params: []
---
Hello!",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let mut fm: *mut c_char = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
    };
    assert!(err.is_null());
    assert!(!tmpl.is_null());

    let fm_str = unsafe { CStr::from_ptr(fm) }.to_str().unwrap();
    assert!(
        fm_str.contains("\"name\":\"static\""),
        "expected name in fm: {fm_str}"
    );
    assert!(
        fm_str.contains("\"params\":[]"),
        "expected empty params in fm: {fm_str}"
    );

    unsafe {
        pt_free_string(fm);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_frontmatter_error() {
    let source = CString::new("no frontmatter at all").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let mut fm: *mut c_char = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_frontmatter(source.as_ptr(), &raw mut tmpl, &raw mut fm)
    };
    assert!(!err.is_null(), "expected error for invalid source");
    assert!(tmpl.is_null(), "template should be null on error");
    unsafe { pt_free_string(err) };
}

// ─── env: support tests ──────────────────────────────────────────────────────

#[test]
fn test_from_source_with_env_basic() {
    let source = CString::new(
        "\
---
env:
  - API_KEY = str

params: [name = str]
---
key={{ API_KEY }} name={{ name }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"API_KEY":"secret123"}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null(), "expected no error, got: {:?}", unsafe {
        err.as_ref().map(|p| CStr::from_ptr(p))
    });
    assert!(!tmpl.is_null());

    let ctx = pt_context_new();
    let key = CString::new("name").unwrap();
    let val = CString::new("Alice").unwrap();
    let err = unsafe { pt_context_set_str(ctx, key.as_ptr(), val.as_ptr()) };
    assert!(err.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null(), "render error");
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "key=secret123 name=Alice");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_int() {
    let source = CString::new(
        "\
---
env:
  - MAX_RETRIES = int

params: []
---
max={{ MAX_RETRIES }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"MAX_RETRIES":5}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "max=5");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_default_override() {
    let source = CString::new(
        "\
---
env:
  - REGION = str := \"us-east-1\"

params: []
---
region={{ REGION }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"REGION":"eu-west-1"}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "region=eu-west-1");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_default_fallback() {
    let source = CString::new(
        "\
---
env:
  - REGION = str := \"us-east-1\"

params: []
---
region={{ REGION }}",
    )
    .unwrap();
    // Empty env — should use default.
    let env_json = CString::new(r"{}").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(err.is_null());

    let ctx = pt_context_new();
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render(tmpl, ctx, &raw mut render_err) };
    assert!(render_err.is_null());
    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(result_str, "region=us-east-1");

    unsafe {
        pt_free_string(result);
        pt_context_free(ctx);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_env_missing_required() {
    let source = CString::new(
        "\
---
env:
  - REQUIRED_KEY = str

params: []
---
{{ REQUIRED_KEY }}",
    )
    .unwrap();
    // Missing required env var — should error at compile time.
    let env_json = CString::new(r"{}").unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_env(source.as_ptr(), env_json.as_ptr(), &raw mut tmpl)
    };
    assert!(!err.is_null(), "expected compile error for missing env");
    assert!(tmpl.is_null());
    let err_str = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
    assert!(
        err_str.contains("REQUIRED_KEY"),
        "error should mention missing key: {err_str}"
    );
    unsafe { pt_free_string(err) };
}

#[test]
fn test_from_source_with_options_combines_env_and_allow_unused() {
    // Declares an unused param and a required env var — both options combined.
    let source = CString::new(
        "\
---
env:
  - REGION = str

params:
  - unused = str := \"x\"
---
region={{ REGION }}",
    )
    .unwrap();
    let env_json = CString::new(r#"{"REGION": "eu-west-1"}"#).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_options(
            source.as_ptr(),
            ptr::null(),
            env_json.as_ptr(),
            true,
            &raw mut tmpl,
        )
    };
    assert!(err.is_null(), "expected compile to succeed");
    assert!(!tmpl.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_empty(tmpl, &raw mut render_err) };
    assert!(render_err.is_null());
    assert_eq!(
        unsafe { CStr::from_ptr(result) }.to_str().unwrap(),
        "region=eu-west-1"
    );

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_from_source_with_options_unused_error_without_allow() {
    // Same template, but allow_unused=false — should fail to compile.
    let source = CString::new(
        "\
---
params: [unused = str]
---
static",
    )
    .unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_options(
            source.as_ptr(),
            ptr::null(),
            ptr::null(),
            false,
            &raw mut tmpl,
        )
    };
    assert!(!err.is_null(), "expected unused-param error");
    assert!(tmpl.is_null());
    unsafe { pt_free_string(err) };
}

#[test]
fn test_from_file_with_options() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opts.tmpl.md");
    std::fs::write(
        &path,
        "\
---
params:
  - unused = str := \"x\"
---
from file",
    )
    .unwrap();

    let path_c = CString::new(path.to_str().unwrap()).unwrap();
    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_file_with_options(
            path_c.as_ptr(),
            ptr::null(),
            ptr::null(),
            true,
            &raw mut tmpl,
        )
    };
    assert!(err.is_null(), "expected compile to succeed");
    assert!(!tmpl.is_null());

    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_empty(tmpl, &raw mut render_err) };
    assert!(render_err.is_null());
    assert_eq!(
        unsafe { CStr::from_ptr(result) }.to_str().unwrap(),
        "from file"
    );

    unsafe {
        pt_free_string(result);
        pt_template_free(tmpl);
    }
}

// ─── cached include + compile-env regression tests ───────────────────────────

/// Write a parent template that includes a `PROMPTS_DIR`-driven child into
/// `dir`, returning the parent source. The child's `env:` var has no default,
/// so it must resolve from the parent's compile-time env on the cached path.
fn write_env_include_fixture(dir: &std::path::Path) -> CString {
    std::fs::write(
        dir.join("child.tmpl.md"),
        "\
---
name: child
env: [PROMPTS_DIR = str]
params: []
---
dir={{ PROMPTS_DIR }}",
    )
    .unwrap();
    CString::new(
        "\
---
params: []
---
> {% include [child](./child.tmpl.md) %}",
    )
    .unwrap()
}

#[test]
fn test_render_cached_include_resolves_compile_env() {
    // Regression: on the cached include path the child's env-only var must
    // resolve from the parent's compile env (mirrors render_ctx_cached). Before
    // the core fix this raised "no value provided and no default".
    let dir = tempfile::tempdir().unwrap();
    let source = write_env_include_fixture(dir.path());
    let base_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let env_json = CString::new(r#"{"PROMPTS_DIR":"/prompts"}"#).unwrap();

    let mut tmpl: *mut PtTemplate = ptr::null_mut();
    let err = unsafe {
        pt_template_from_source_with_options(
            source.as_ptr(),
            base_dir.as_ptr(),
            env_json.as_ptr(),
            false,
            &raw mut tmpl,
        )
    };
    assert!(err.is_null(), "compile error: {:?}", unsafe {
        err.as_ref().map(|p| CStr::from_ptr(p))
    });

    let cache = pt_cache_new();
    let ctx = pt_context_new();

    // First render compiles the include from disk with env threaded in.
    let mut render_err: *mut c_char = ptr::null_mut();
    let result = unsafe { pt_template_render_cached(tmpl, ctx, cache, &raw mut render_err) };
    assert!(render_err.is_null(), "render error: {:?}", unsafe {
        render_err.as_ref().map(|p| CStr::from_ptr(p))
    });
    let first = unsafe { CStr::from_ptr(result) }
        .to_str()
        .unwrap()
        .to_string();
    assert!(first.contains("dir=/prompts"), "got: {first}");

    // Second render is served from cache; the env-injected const persists.
    let result2 = unsafe { pt_template_render_cached(tmpl, ctx, cache, &raw mut render_err) };
    assert!(render_err.is_null());
    assert_eq!(unsafe { CStr::from_ptr(result2) }.to_str().unwrap(), first);

    unsafe {
        pt_free_string(result);
        pt_free_string(result2);
        pt_context_free(ctx);
        pt_cache_free(cache);
        pt_template_free(tmpl);
    }
}

#[test]
fn test_render_cached_include_invalidated_on_env_change() {
    // Regression: a shared cache must not serve a stale include when only the
    // compile env changed (file mtime + content are identical, which would
    // otherwise hit the fast path).
    let dir = tempfile::tempdir().unwrap();
    let source = write_env_include_fixture(dir.path());
    let base_dir = CString::new(dir.path().to_str().unwrap()).unwrap();
    let cache = pt_cache_new();
    let ctx = pt_context_new();

    let render_with = |env_json: &str| -> String {
        let env = CString::new(env_json).unwrap();
        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err = unsafe {
            pt_template_from_source_with_options(
                source.as_ptr(),
                base_dir.as_ptr(),
                env.as_ptr(),
                false,
                &raw mut tmpl,
            )
        };
        assert!(err.is_null(), "compile error: {:?}", unsafe {
            err.as_ref().map(|p| CStr::from_ptr(p))
        });
        let mut render_err: *mut c_char = ptr::null_mut();
        let result = unsafe { pt_template_render_cached(tmpl, ctx, cache, &raw mut render_err) };
        assert!(render_err.is_null(), "render error: {:?}", unsafe {
            render_err.as_ref().map(|p| CStr::from_ptr(p))
        });
        let out = unsafe { CStr::from_ptr(result) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe {
            pt_free_string(result);
            pt_template_free(tmpl);
        }
        out
    };

    assert!(render_with(r#"{"PROMPTS_DIR":"/alpha"}"#).contains("dir=/alpha"));
    // Same file + cache, different env — must recompute, not serve /alpha.
    assert!(render_with(r#"{"PROMPTS_DIR":"/beta"}"#).contains("dir=/beta"));

    unsafe {
        pt_context_free(ctx);
        pt_cache_free(cache);
    }
}
