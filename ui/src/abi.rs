//! Unicode NSIS stack boundary. Nodes use the NSIS allocator, never Rust's heap.
use std::{mem::size_of, ptr::*};
use windows_sys::Win32::{
    Foundation::GlobalFree,
    System::Memory::{GPTR, GlobalAlloc},
};

#[repr(C)]
pub struct Stack {
    next: *mut Stack,
    text: [u16; 1],
}

pub(super) unsafe fn pop(stack: *mut *mut Stack, capacity: usize) -> String {
    if stack.is_null() || (*stack).is_null() {
        return String::new();
    }
    let node = *stack;
    let text = std::slice::from_raw_parts(addr_of!((*node).text).cast::<u16>(), capacity);
    let end = text.iter().position(|c| *c == 0).unwrap_or(capacity);
    let value = String::from_utf16_lossy(&text[..end]);
    *stack = (*node).next;
    GlobalFree(node.cast());
    value
}

pub(super) unsafe fn push(stack: *mut *mut Stack, capacity: usize, value: &str) {
    if stack.is_null() || capacity == 0 {
        return;
    }
    let Some(bytes) = capacity
        .checked_mul(2)
        .and_then(|n| n.checked_add(size_of::<*mut Stack>()))
    else {
        return;
    };
    let node = GlobalAlloc(GPTR, bytes).cast::<Stack>();
    if node.is_null() {
        return;
    }
    (*node).next = *stack;
    let output = std::slice::from_raw_parts_mut(addr_of_mut!((*node).text).cast::<u16>(), capacity);
    for (slot, code) in output[..capacity - 1].iter_mut().zip(value.encode_utf16()) {
        *slot = code;
    }
    *stack = node;
}

pub(super) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
