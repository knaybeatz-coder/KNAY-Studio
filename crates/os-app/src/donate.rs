//! Optional support link; opens the user's browser without initiating a payment.
pub const URL: &str = "https://paypal.me/michaellanemusic";

pub fn open() -> Result<(), String> { open_url(URL) }

pub fn open_url(address: &str) -> Result<(), String> {
    if cfg!(test) { return Ok(()); }
    #[cfg(windows)] {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let url: Vec<u16> = address.encode_utf16().chain(Some(0)).collect();
        let result = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), url.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
        if result as isize <= 32 { return Err(format!("Could not open your browser. Visit {address}")); }
    }
    #[cfg(not(windows))] {
        let command = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(command).arg(address).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}
