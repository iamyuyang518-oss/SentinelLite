//! Minimal COM apartment guard shared by on-demand read-only Windows queries.

use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

pub(crate) struct ComApartment {
    uninitialize: bool,
}

impl ComApartment {
    pub(crate) fn initialize() -> Result<Self, u32> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(Self { uninitialize: true })
        } else if result.0 as u32 == 0x8001_0106 {
            // The worker thread is already initialized in another apartment. COM is usable, but
            // this call did not acquire a reference that we may release.
            Ok(Self {
                uninitialize: false,
            })
        } else {
            Err(result.0 as u32)
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}
