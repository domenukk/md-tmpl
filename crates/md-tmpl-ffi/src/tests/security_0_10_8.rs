//! C FFI 0.10.8 security & robustness regression tests (`M-H6`, `M-H7`, `M-L2`).

use std::{
    ffi::{CStr, CString, c_char},
    ptr,
};

use md_tmpl::Value;

use crate::{
    ERR_KIND_SEP, PtTemplate, err_to_cstring, metadata::value_to_json, pt_context_free,
    pt_context_new, pt_context_set_str, pt_fence, pt_free_string, pt_is_quarantined,
    pt_template_declarations, pt_template_free, pt_template_from_source,
    pt_template_from_source_with_frontmatter, pt_template_render, pt_template_render_json,
    pt_template_validate_declarations, pt_unquarantine,
};

unsafe fn take_cstr(ptr: *mut c_char) -> String {
    assert!(!ptr.is_null(), "expected non-null C string");
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("valid UTF-8")
        .to_owned();
    unsafe { pt_free_string(ptr) };
    s
}

#[test]
fn ffi_null_out_pointers_never_dereference_null() {
    let src = CString::new(
        r"---
params: []
---
ok",
    )
    .unwrap();

    unsafe {
        let err = take_cstr(pt_template_from_source(src.as_ptr(), ptr::null_mut()));
        assert!(err.contains("null out pointer"));

        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        let err_fm = take_cstr(pt_template_from_source_with_frontmatter(
            src.as_ptr(),
            &raw mut tmpl,
            ptr::null_mut(),
        ));
        assert!(err_fm.contains("null out pointer"));
        assert!(tmpl.is_null());

        // Null `out_err` returns null safely without writing through null.
        assert!(pt_template_render(ptr::null(), ptr::null(), ptr::null_mut()).is_null());
        assert!(pt_fence(b"hi".as_ptr(), 2, ptr::null(), ptr::null_mut()).is_null());
    }
}

#[test]
fn ffi_fence_strips_nul_before_counting_backticks() {
    let payload = b"``\x00``\nESCAPED";
    let mut err: *mut c_char = ptr::null_mut();
    unsafe {
        let fenced = take_cstr(pt_fence(
            payload.as_ptr(),
            payload.len(),
            ptr::null(),
            &raw mut err,
        ));
        assert!(err.is_null());
        assert_eq!(fenced, "`````\n````\nESCAPED\n`````");
    }
}

#[test]
fn ffi_reserved_kind_key_and_err_sep_spoofing_are_safe() {
    let key_kind = CString::new("__kind__").unwrap();
    let val_admin = CString::new("Admin").unwrap();
    let json_kind = CString::new(r#"{"__kind__":"Admin"}"#).unwrap();
    let src = CString::new(
        r"---
params: []
---
ok",
    )
    .unwrap();

    unsafe {
        let ctx = pt_context_new();
        let err = take_cstr(pt_context_set_str(
            ctx,
            key_kind.as_ptr(),
            val_admin.as_ptr(),
        ));
        assert!(err.starts_with("syntax\u{1f}"));
        assert!(err.contains("__kind__"));
        pt_context_free(ctx);

        let mut tmpl: *mut PtTemplate = ptr::null_mut();
        assert!(pt_template_from_source(src.as_ptr(), &raw mut tmpl).is_null());
        let mut r_err: *mut c_char = ptr::null_mut();
        assert!(pt_template_render_json(tmpl, json_kind.as_ptr(), true, &raw mut r_err).is_null());
        let r_msg = take_cstr(r_err);
        assert!(r_msg.contains("__kind__"));
        pt_template_free(tmpl);

        // Plain error strings must neutralize embedded U+001F so they cannot spoof kind framing.
        let spoofed = take_cstr(err_to_cstring("missing_params\u{1f}fake"));
        assert!(!spoofed.contains(ERR_KIND_SEP));
    }
}

#[test]
fn ffi_unquarantine_invalid_utf8_and_validate_declarations_trust_downgrade() {
    let bad_env = b"<untrusted_content>\n\xff\n</untrusted_content>";
    unsafe {
        assert!(!pt_is_quarantined(
            bad_env.as_ptr(),
            bad_env.len(),
            ptr::null()
        ));
        assert!(pt_unquarantine(bad_env.as_ptr(), bad_env.len(), ptr::null()).is_null());
    }

    // Non-finite floats serialize to valid JSON `null`.
    assert_eq!(value_to_json(&Value::Float(f64::NAN)), "null");
    assert_eq!(value_to_json(&Value::Float(f64::INFINITY)), "null");
    assert_eq!(value_to_json(&Value::Float(-0.0)), "0");

    let safe_src = CString::new(
        r"---
params:
  - x = str
---
{{ x }}",
    )
    .unwrap();
    let downgraded_src = CString::new(
        r"---
params:
  - x = trusted str
---
{{ x }}",
    )
    .unwrap();

    unsafe {
        let mut safe_t: *mut PtTemplate = ptr::null_mut();
        let mut down_t: *mut PtTemplate = ptr::null_mut();
        assert!(pt_template_from_source(safe_src.as_ptr(), &raw mut safe_t).is_null());
        assert!(pt_template_from_source(downgraded_src.as_ptr(), &raw mut down_t).is_null());

        let decls = take_cstr(pt_template_declarations(safe_t));
        let decls_c = CString::new(decls).unwrap();
        assert!(pt_template_validate_declarations(safe_t, decls_c.as_ptr()).is_null());

        let diff_err = take_cstr(pt_template_validate_declarations(down_t, decls_c.as_ptr()));
        assert!(diff_err.contains("trusted str"));

        pt_template_free(safe_t);
        pt_template_free(down_t);
    }
}
