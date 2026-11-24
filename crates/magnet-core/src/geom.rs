/// A rectangle in physical screen pixels, same layout as Win32 `RECT`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }

    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub const fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    pub const fn contains(&self, other: &Rect) -> bool {
        self.left <= other.left && self.top <= other.top && self.right >= other.right && self.bottom >= other.bottom
    }
}

/// Distance between a window's outer rect (what `GetWindowRect` and
/// `WM_MOVING` report) and its visible frame (DWM extended frame bounds).
///
/// On Windows 10/11 top-level windows have invisible resize borders, so
/// snapping the raw window rect would leave ~7px gaps between windows.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub fn between(outer: Rect, inner: Rect) -> Self {
        Self {
            left: (inner.left - outer.left).max(0),
            top: (inner.top - outer.top).max(0),
            right: (outer.right - inner.right).max(0),
            bottom: (outer.bottom - inner.bottom).max(0),
        }
    }

    /// Outer window rect -> visible frame.
    pub const fn shrink(&self, r: Rect) -> Rect {
        Rect::new(r.left + self.left, r.top + self.top, r.right - self.right, r.bottom - self.bottom)
    }

    /// Visible frame -> outer window rect.
    pub const fn grow(&self, r: Rect) -> Rect {
        Rect::new(r.left - self.left, r.top - self.top, r.right + self.right, r.bottom + self.bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insets_round_trip() {
        let outer = Rect::new(93, 100, 507, 407);
        let inner = Rect::new(100, 100, 500, 400);
        let i = Insets::between(outer, inner);
        assert_eq!(i, Insets { left: 7, top: 0, right: 7, bottom: 7 });
        assert_eq!(i.shrink(outer), inner);
        assert_eq!(i.grow(inner), outer);
    }
}
