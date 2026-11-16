//! A thread of this library settles the browser agent's answers, under Ash and Emscripten alike:
//! the program's thread may be waiting on one of them, so they are not left to it. In a
//! threaded Emscripten build the web page also starts the agent (`c/agent.c`), which Ash's
//! page does itself.

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering::SeqCst};

#[cfg(target_os = "emscripten")]
unsafe extern "C" {
    fn hxn_gpu_webgpu_agent(address: u32) -> i32;
}

/// The word the browser agent adds one to, and notifies, after settling a reply.
static WAKE: AtomicI32 = AtomicI32::new(0);

#[cfg(target_os = "emscripten")]
pub unsafe fn ash_host_agent(_name: *const u8, _name_len: u32, address: u32) -> i32 {
    unsafe { hxn_gpu_webgpu_agent(address) }
}

pub unsafe fn ash_host_watch(
    _word: *const u32,
    handler: unsafe extern "C" fn(*mut c_void),
    context: *mut c_void,
) -> *const u32 {
    let context = context as usize;
    let watching = std::thread::Builder::new()
        .name(String::from("gpu_webgpu answers"))
        .spawn(move || {
            let mut seen = 0;
            loop {
                unsafe { core::arch::wasm32::memory_atomic_wait32(WAKE.as_ptr(), seen, -1) };
                let now = WAKE.load(SeqCst);
                if now != seen {
                    seen = now;
                    unsafe { handler(context as *mut c_void) };
                }
            }
        });
    match watching {
        Ok(_) => WAKE.as_ptr().cast_const().cast(),
        Err(_) => std::ptr::null(),
    }
}
