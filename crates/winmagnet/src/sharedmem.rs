use std::ptr::null;

use magnet_core::shared::{SHARED_NAME, SharedBlock, Snapshot};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
};

use crate::util::wide;

/// Owner side of the shared-memory block read by the hook DLL.
pub struct SharedMemory {
    handle: HANDLE,
    block: *mut SharedBlock,
}

impl SharedMemory {
    pub fn create() -> Result<Self, String> {
        let name = wide(SHARED_NAME);
        let size = size_of::<SharedBlock>();
        // SAFETY: plain Win32 calls with valid arguments.
        unsafe {
            let handle =
                CreateFileMappingW(INVALID_HANDLE_VALUE, null(), PAGE_READWRITE, 0, size as u32, name.as_ptr());
            if handle.is_null() {
                return Err("Could not create the shared memory block.".into());
            }
            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size);
            if view.Value.is_null() {
                CloseHandle(handle);
                return Err("Could not map the shared memory block.".into());
            }
            let block = view.Value.cast::<SharedBlock>();
            SharedBlock::init(block);
            Ok(Self { handle, block })
        }
    }

    pub fn publish(&self, snapshot: &Snapshot) -> u32 {
        // SAFETY: block is our live, writable mapping.
        unsafe { SharedBlock::publish(self.block, snapshot) }
    }

    pub fn clear_target(&self) {
        self.publish(&Snapshot::empty());
    }

    pub fn set_enabled(&self, on: bool) {
        // SAFETY: block is our live mapping.
        unsafe { (*self.block).enabled.store(on as u32, std::sync::atomic::Ordering::Release) };
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        // SAFETY: releasing what `create` acquired.
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.block.cast() });
            CloseHandle(self.handle);
        }
    }
}
