pub mod replay;
pub mod runner;
pub mod signal;

use std::alloc::{Layout, alloc, dealloc};

use serde_json::json;

pub use runner::{DistrictPackage, SmOp, SmRequest, handle_request};

pub fn invoke_json(input: &str) -> String {
    let parsed: Result<SmRequest, String> =
        serde_json::from_str(input).map_err(|e| format!("invalid sm request JSON: {e}"));
    let value = match parsed.and_then(handle_request) {
        Ok(v) => v,
        Err(err) => json!({ "ok": false, "error": err }),
    };
    serde_json::to_string(&value).unwrap_or_else(|_| "{\"ok\":false,\"error\":\"encode\"}".into())
}

#[unsafe(no_mangle)]
pub extern "C" fn sm_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let Ok(layout) = Layout::from_size_align(len, 1) else {
        return std::ptr::null_mut();
    };
    unsafe { alloc(layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn sm_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(len, 1) {
        unsafe { dealloc(ptr, layout) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn sm_invoke(req_ptr: *const u8, req_len: usize) -> u64 {
    let req_bytes = if req_ptr.is_null() || req_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(req_ptr, req_len) }
    };
    let req_str = String::from_utf8_lossy(req_bytes);
    let resp = invoke_json(&req_str);
    let bytes = resp.into_bytes();
    let len = bytes.len();
    let out_ptr = sm_alloc(len);
    if !out_ptr.is_null() && len > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, len);
        }
    }
    ((out_ptr as u64) << 32) | (len as u64)
}
