//! A Windows "event": a flag one side sets and another thread sleeps on.
//! WASAPI sets it each time the audio device is ready for more work, so our
//! threads wake exactly when needed instead of guessing with `sleep`.

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::{PCWSTR, Result};

pub struct Event(HANDLE);

impl Event {
    pub fn new() -> Result<Self> {
        // Auto-reset: waking a waiter clears the flag again.
        unsafe { CreateEventW(None, false, false, PCWSTR::null()).map(Self) }
    }

    pub fn handle(&self) -> HANDLE {
        self.0
    }

    /// Sleep until the event fires. Returns false if `timeout_ms` passed first.
    pub fn wait(&self, timeout_ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.0, timeout_ms) == WAIT_OBJECT_0 }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
