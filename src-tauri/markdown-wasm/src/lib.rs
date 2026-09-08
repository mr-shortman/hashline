//! Byte interface for the WebAssembly build.
//!
//! `alloc` → write UTF-8 source → `parse` → read `result_ptr()`/`result_len()`
//! → `free`. The result is the same packet the desktop receives over IPC, minus
//! the file metadata fields; see `packet.rs`.

use std::alloc::{alloc as raw_alloc, dealloc, Layout};
use std::cell::RefCell;

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// # Safety
/// The caller must pass the returned pointer and the same length to `free`.
#[no_mangle]
pub unsafe extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::dangling().as_ptr();
    }
    raw_alloc(Layout::from_size_align_unchecked(len, 1))
}

/// # Safety
/// `ptr` must come from `alloc` with the same `len`.
#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
    if len != 0 {
        dealloc(ptr, Layout::from_size_align_unchecked(len, 1));
    }
}

/// Parses `len` UTF-8 bytes at `ptr` and returns the packet length. Invalid
/// UTF-8 yields an empty result.
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn parse(ptr: *const u8, len: usize) -> usize {
    let source = match std::str::from_utf8(std::slice::from_raw_parts(ptr, len)) {
        Ok(source) => source,
        Err(_) => return 0,
    };
    let packet = hashline_markdown::to_packet(&hashline_markdown::parse(source), "");
    let length = packet.len();
    RESULT.with(|result| *result.borrow_mut() = packet);
    length
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 {
    RESULT.with(|result| result.borrow().as_ptr())
}
