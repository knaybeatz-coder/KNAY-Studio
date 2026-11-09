// SPDX-License-Identifier: GPL-3.0-or-later
//! Operating-system file actions from the Browser's sample menu.
use std::path::Path;

pub(crate) fn trash(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("This Browser item is not a local file".into());
    }
    #[cfg(windows)]
    {
        let path = path.to_owned();
        win::apartment(move || unsafe {
            use windows::Win32::{
                System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
                UI::Shell::*,
            };
            let wide = win::wide(&path);
            let item: IShellItem =
                SHCreateItemFromParsingName(windows::core::PCWSTR(wide.as_ptr()), None)?;
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            op.SetOperationFlags(FOFX_RECYCLEONDELETE | FOF_ALLOWUNDO)?;
            op.DeleteItem(&item, None)?;
            op.PerformOperations()?;
            if op.GetAnyOperationsAborted()?.as_bool() {
                return Err(windows::core::Error::new(
                    windows::Win32::Foundation::E_ABORT,
                    "Move to Recycle Bin cancelled",
                ));
            }
            Ok(())
        })?;
    }
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "tell application \"Finder\" to delete (POSIX file (item 1 of argv) as alias)",
                "-e",
                "end run",
                "--",
            ])
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Could not move sample to Trash".into());
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let status = std::process::Command::new("gio")
            .args(["trash", "--"])
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Could not move sample to Trash".into());
        }
    }
    if path.exists() {
        return Err("The sample is still in its original folder".into());
    }
    Ok(())
}

#[cfg(windows)]
mod win {
    use std::{os::windows::ffi::OsStrExt, path::Path};
    use windows::Win32::System::Com::*;
    pub(super) fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    pub(super) fn apartment(f: impl FnOnce() -> windows::core::Result<()>) -> Result<(), String> {
        // Run the native popup on the window thread so shell messages can reach
        // its owner while TrackPopupMenu is pumping the modal message loop.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
            let result = f();
            CoUninitialize();
            result.map_err(|e| e.to_string())
        }
    }
}

#[cfg(windows)]
pub(crate) fn shell_menu(path: &Path) -> Result<(), String> {
    use windows::core::{PCSTR, PCWSTR};
    use windows::Win32::{
        Foundation::POINT,
        System::Com::CoTaskMemFree,
        UI::{Shell::*, WindowsAndMessaging::*},
    };
    if !path.is_file() {
        return Err("Locate the sample before opening its shell menu".into());
    }
    let wide = win::wide(path);
    win::apartment(move || unsafe {
        let owner = GetForegroundWindow();
        let mut full = std::ptr::null_mut();
        SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut full, 0, None)?;
        let result = (|| {
            let mut child = std::ptr::null_mut();
            let folder: IShellFolder = SHBindToParent(full, Some(&mut child))?;
            let context: IContextMenu = folder.GetUIObjectOf(owner, &[child as *const _], None)?;
            let menu = CreatePopupMenu()?;
            let result = (|| {
                context
                    .QueryContextMenu(menu, 0, 1, 0x7fff, CMF_NORMAL)
                    .ok()?;
                let mut pos = POINT::default();
                GetCursorPos(&mut pos)?;
                let id = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    pos.x,
                    pos.y,
                    Some(0),
                    owner,
                    None,
                )
                .0;
                if id > 0 {
                    let info = CMINVOKECOMMANDINFO {
                        cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                        hwnd: owner,
                        lpVerb: PCSTR((id - 1) as usize as *const u8),
                        nShow: 1,
                        ..Default::default()
                    };
                    context.InvokeCommand(&info)?;
                }
                Ok(())
            })();
            let _ = DestroyMenu(menu);
            result
        })();
        CoTaskMemFree(Some(full.cast()));
        result
    })
}
