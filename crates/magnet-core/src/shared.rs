//! Layout of the named shared-memory block written by the app and read by the
//! hook DLL inside the dragged window's process.
//!
//! The app publishes a [`Snapshot`] (the target window plus every edge it can
//! snap to) right before it hooks the target thread, so the hook never has to
//! enumerate windows itself. Writes are protected by a seqlock: `seq` is odd
//! while a write is in progress and its even value doubles as the session id.

use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicU32, Ordering, fence};

use crate::config::Config;
use crate::geom::{Insets, Rect};

pub const SHARED_NAME: &str = "Local\\WinMagnet.Shared.v1";
pub const MAGIC: u32 = u32::from_le_bytes(*b"WMAG");
pub const VERSION: u32 = 1;
pub const MAX_WINDOWS: usize = 256;
pub const MAX_MONITORS: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MonitorRects {
    pub bounds: Rect,
    pub work: Rect,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    /// HWND of the window being moved/resized.
    pub target_hwnd: u64,
    /// Outer rect -> visible frame of the target, in physical pixels.
    pub insets: Insets,
    /// Config already scaled to physical pixels for the target's monitor.
    pub config: Config,
    pub window_count: u32,
    pub monitor_count: u32,
    /// Visible frames of the other windows, top of the z-order first.
    pub windows: [Rect; MAX_WINDOWS],
    pub monitors: [MonitorRects; MAX_MONITORS],
}

impl Snapshot {
    pub fn empty() -> Self {
        Self {
            target_hwnd: 0,
            insets: Insets::default(),
            config: Config::DEFAULT,
            window_count: 0,
            monitor_count: 0,
            windows: [Rect::default(); MAX_WINDOWS],
            monitors: [MonitorRects::default(); MAX_MONITORS],
        }
    }

    pub fn windows(&self) -> &[Rect] {
        &self.windows[..(self.window_count as usize).min(MAX_WINDOWS)]
    }

    pub fn monitors(&self) -> &[MonitorRects] {
        &self.monitors[..(self.monitor_count as usize).min(MAX_MONITORS)]
    }

    /// Returns false when full.
    pub fn push_window(&mut self, r: Rect) -> bool {
        let n = self.window_count as usize;
        if n >= MAX_WINDOWS {
            return false;
        }
        self.windows[n] = r;
        self.window_count += 1;
        true
    }

    pub fn push_monitor(&mut self, m: MonitorRects) -> bool {
        let n = self.monitor_count as usize;
        if n >= MAX_MONITORS {
            return false;
        }
        self.monitors[n] = m;
        self.monitor_count += 1;
        true
    }
}

#[repr(C)]
pub struct SharedBlock {
    pub magic: u32,
    pub version: u32,
    /// Master switch toggled from the tray.
    pub enabled: AtomicU32,
    seq: AtomicU32,
    snapshot: Snapshot,
}

impl SharedBlock {
    /// Initialise a freshly mapped, zeroed block.
    ///
    /// # Safety
    /// `this` must point to a writable mapping of at least `size_of::<SharedBlock>()` bytes.
    pub unsafe fn init(this: *mut SharedBlock) {
        unsafe {
            addr_of_mut!((*this).magic).write_volatile(MAGIC);
            addr_of_mut!((*this).version).write_volatile(VERSION);
            (*this).enabled.store(1, Ordering::Release);
            (*this).seq.store(0, Ordering::Release);
        }
    }

    pub fn is_valid(&self) -> bool {
        // SAFETY: plain reads of fields in a live mapping.
        unsafe { addr_of!(self.magic).read_volatile() == MAGIC && addr_of!(self.version).read_volatile() == VERSION }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire) != 0
    }

    /// Current session id (even), or `None` while a write is in progress or
    /// before anything has been published.
    pub fn session(&self) -> Option<u32> {
        let s = self.seq.load(Ordering::Acquire);
        (s != 0 && s & 1 == 0).then_some(s)
    }

    /// Cheap peek at the target HWND without copying the whole snapshot.
    pub fn target_hwnd(&self) -> u64 {
        // SAFETY: aligned u64 read from a live mapping; torn reads are
        // caught later by `read`.
        unsafe { addr_of!(self.snapshot.target_hwnd).read_volatile() }
    }

    /// Publish a new snapshot (single writer: the app).
    ///
    /// # Safety
    /// `this` must point to an initialised, writable block.
    pub unsafe fn publish(this: *mut SharedBlock, snap: &Snapshot) -> u32 {
        unsafe {
            let seq = &(*this).seq;
            let s = seq.load(Ordering::Relaxed) & !1;
            seq.store(s.wrapping_add(1), Ordering::Relaxed);
            fence(Ordering::Release);
            addr_of_mut!((*this).snapshot).write_volatile(*snap);
            let next = match s.wrapping_add(2) {
                0 => 2,
                n => n,
            };
            seq.store(next, Ordering::Release);
            next
        }
    }

    /// Consistent copy of the latest snapshot plus its session id.
    pub fn read(&self) -> Option<(u32, Box<Snapshot>)> {
        for _ in 0..8 {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 == 0 {
                return None;
            }
            if s1 & 1 == 1 {
                core::hint::spin_loop();
                continue;
            }
            // SAFETY: volatile copy out of shared memory; validated by the
            // seqlock check below.
            let snap = Box::new(unsafe { addr_of!(self.snapshot).read_volatile() });
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == s1 {
                return Some((s1, snap));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    #[test]
    fn publish_and_read() {
        let mut storage = Box::new(MaybeUninit::<SharedBlock>::zeroed());
        let block = storage.as_mut_ptr();
        unsafe { SharedBlock::init(block) };
        let shared = unsafe { &*block };
        assert!(shared.is_valid());
        assert!(shared.read().is_none());

        let mut snap = Snapshot::empty();
        snap.target_hwnd = 0x1234;
        snap.push_window(Rect::new(1, 2, 3, 4));
        let id = unsafe { SharedBlock::publish(block, &snap) };

        let (sid, copy) = shared.read().unwrap();
        assert_eq!(sid, id);
        assert_eq!(shared.session(), Some(id));
        assert_eq!(shared.target_hwnd(), 0x1234);
        assert_eq!(copy.windows(), &[Rect::new(1, 2, 3, 4)]);

        let id2 = unsafe { SharedBlock::publish(block, &snap) };
        assert_ne!(id, id2);
    }
}
