//! Template cache lifecycle FFI.

use std::{ffi::c_char, path::Path, ptr, sync::Arc};

use md_tmpl::TemplateCache;

use crate::{
    ERR_NULL_CACHE, ERR_NULL_OUT_POINTER, PtCache, PtTemplate, catch_ffi_err, catch_ffi_val,
    cstr_to_str, err_to_cstring, terr_to_cstring,
};

/// Create a new template cache.
#[unsafe(no_mangle)]
pub extern "C" fn pt_cache_new() -> *mut PtCache {
    catch_ffi_val(ptr::null_mut(), || {
        Box::into_raw(Box::new(PtCache {
            inner: Arc::new(TemplateCache::new()),
        }))
    })
}

/// Free a cache handle.
///
/// # Safety
///
/// `cache` must have been returned by `pt_cache_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_free(cache: *mut PtCache) {
    catch_ffi_val((), || {
        if !cache.is_null() {
            drop(unsafe { Box::from_raw(cache) });
        }
    });
}

/// Load a template through the cache.
///
/// # Safety
///
/// - `cache` must be a valid cache handle.
/// - `path` must be a valid NUL-terminated UTF-8 file path.
/// - `out` must be a valid pointer to a `*mut PtTemplate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_load(
    cache: *const PtCache,
    path: *const c_char,
    out: *mut *mut PtTemplate,
) -> *mut c_char {
    catch_ffi_err(|| {
        if out.is_null() {
            return err_to_cstring(ERR_NULL_OUT_POINTER);
        }
        let Some(cache) = (unsafe { cache.as_ref() }) else {
            return err_to_cstring(ERR_NULL_CACHE);
        };
        let path_str = match unsafe { cstr_to_str(path) } {
            Ok(s) => s,
            Err(e) => return err_to_cstring(&e),
        };
        match cache.inner.load(Path::new(path_str)) {
            Ok(tmpl) => {
                let handle = Box::new(PtTemplate { inner: tmpl });
                unsafe { *out = Box::into_raw(handle) };
                ptr::null_mut()
            }
            Err(e) => terr_to_cstring(&e),
        }
    })
}

/// Clear all entries from the cache.
///
/// # Safety
///
/// `cache` must be a valid cache handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_clear(cache: *const PtCache) {
    catch_ffi_val((), || {
        if let Some(c) = unsafe { cache.as_ref() } {
            c.inner.clear();
        }
    });
}

/// Return the number of cached templates.
///
/// # Safety
///
/// `cache` must be a valid cache handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_template_count(cache: *const PtCache) -> usize {
    catch_ffi_val(0, || match unsafe { cache.as_ref() } {
        Some(c) => c.inner.template_count(),
        None => 0,
    })
}

/// Return the number of cached includes.
///
/// # Safety
///
/// `cache` must be a valid cache handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pt_cache_include_count(cache: *const PtCache) -> usize {
    catch_ffi_val(0, || match unsafe { cache.as_ref() } {
        Some(c) => c.inner.include_count(),
        None => 0,
    })
}
