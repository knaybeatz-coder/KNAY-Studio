// SPDX-License-Identifier: GPL-3.0-or-later
//! Host-driven VST3 editor resizing; sizes are pixels except for macOS view points.
use vst3_sys::base::{kResultOk, kResultTrue};
use vst3_sys::gui::{IPlugView, ViewRect};

pub(crate) fn resize(view: &dyn IPlugView, width: u32, height: u32, scale: f32) -> Option<(i32, i32)> {
    if width == 0 || height == 0 || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    unsafe {
        if view.can_resize() != kResultTrue { return None; }
        let mut rect = ViewRect { left: 0, top: 0,
            right: (width as f64 / f64::from(scale)).round().clamp(1.0, i32::MAX as f64) as i32,
            bottom: (height as f64 / f64::from(scale)).round().clamp(1.0, i32::MAX as f64) as i32 };
        // Constraints are optional. An unsupported implementation leaves the proposed rectangle intact.
        view.check_size_constraint(&mut rect);
        dimensions(rect)?;
        if view.on_size(&mut rect) != kResultOk { return None; }
        let mut actual = rect;
        if view.get_size(&mut actual) == kResultOk {
            dimensions(actual)
        } else {
            dimensions(rect)
        }
    }
}

fn dimensions(rect: ViewRect) -> Option<(i32, i32)> {
    let width = rect.right.checked_sub(rect.left)?;
    let height = rect.bottom.checked_sub(rect.top)?;
    (width > 0 && height > 0).then_some((width, height))
}

#[cfg(test)]
mod tests {
    #![allow(semicolon_in_expressions_from_non_local_macros)]
    use super::*;
    use std::cell::Cell;
    use vst3_sys as vst3_com;
    use vst3_sys::{c_void, VST3};
    use vst3_sys::base::{char16, tresult, FIDString, TBool, kResultFalse};
    use crate::host::Com;

    #[VST3(implements(IPlugView))]
    struct View { size: Cell<ViewRect>, resizable: Cell<bool>, reject: Cell<bool>, calls: Cell<u32> }
    impl IPlugView for View {
        unsafe fn is_platform_type_supported(&self, _: FIDString) -> tresult { kResultOk }
        unsafe fn attached(&self, _: *mut c_void, _: FIDString) -> tresult { kResultOk }
        unsafe fn removed(&self) -> tresult { kResultOk }
        unsafe fn on_wheel(&self, _: f32) -> tresult { kResultOk }
        unsafe fn on_key_down(&self, _: char16, _: i16, _: i16) -> tresult { kResultOk }
        unsafe fn on_key_up(&self, _: char16, _: i16, _: i16) -> tresult { kResultOk }
        unsafe fn get_size(&self, size: *mut ViewRect) -> tresult { unsafe { *size = self.size.get(); } kResultOk }
        unsafe fn on_size(&self, rect: *mut ViewRect) -> tresult {
            self.calls.set(self.calls.get()+1);
            if self.reject.get() { return kResultFalse; }
            self.size.set(unsafe { *rect }); kResultOk
        }
        unsafe fn on_focus(&self, _: TBool) -> tresult { kResultOk }
        unsafe fn set_frame(&self, _: *mut c_void) -> tresult { kResultOk }
        unsafe fn can_resize(&self) -> tresult { if self.resizable.get() { kResultTrue } else { kResultFalse } }
        unsafe fn check_size_constraint(&self, rect: *mut ViewRect) -> tresult {
            let r = unsafe { &mut *rect };
            r.right = r.right.clamp(640, 1600); r.bottom = r.bottom.clamp(360, 1000); kResultOk
        }
    }

    #[test]
    fn vst3_editor_resize_respects_constraints_scale_and_refusal() {
        // No native windows, devices, or third-party GUI are opened.
        let view = Com::new(View::allocate(Cell::new(ViewRect::default()), Cell::new(true), Cell::new(false), Cell::new(0)));
        let v = view.get();
        assert_eq!(resize(v, 1100, 700, 1.0), Some((1100, 700)));
        assert_eq!(resize(v, 300, 100, 1.0), Some((640, 360)));
        assert_eq!(resize(v, 3000, 2000, 1.0), Some((1600, 1000)));
        assert_eq!(resize(v, 1800, 1200, 2.0), Some((900, 600)));
        v.reject.set(true);
        assert_eq!(resize(v, 1000, 600, 1.0), None);
        assert_eq!((v.size.get().right, v.size.get().bottom), (900, 600));
        v.resizable.set(false);
        let calls = v.calls.get();
        assert_eq!(resize(v, 1100, 700, 1.0), None);
        assert_eq!(resize(v, 0, 700, 1.0), None);
        assert_eq!(resize(v, 1100, 700, f32::NAN), None);
        assert_eq!(v.calls.get(), calls);
    }
}
