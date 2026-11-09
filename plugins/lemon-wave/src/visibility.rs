//! Read the active host profile on the editor thread. These flags affect only presentation.
pub const MODULATION_MENU: &str = "visible_wave-mod-menu";
pub const HARMONIC_CONTROLS: &str = "visible_wave-harmonics";

pub fn shown(index: usize) -> bool {
    if cfg!(test) { return true; }
    use std::{cell::RefCell, time::{Duration, Instant}};
    thread_local! {
        static CACHED: RefCell<Option<(Instant, [bool; 2])>> = const { RefCell::new(None) };
    }
    CACHED.with(|cached| {
        let mut cached = cached.borrow_mut();
        if cached.as_ref().is_none_or(|(when, _)| when.elapsed() >= Duration::from_millis(300)) {
            let mut flags = [true; 2];
            if let Some(text) = os_paths::user_file("settings.txt").and_then(|path| std::fs::read_to_string(path).ok()) {
                for (name, value) in text.lines().filter_map(|line| line.split_once('=')) {
                    match name.trim() {
                        MODULATION_MENU => flags[0] = value.trim() != "0",
                        HARMONIC_CONTROLS => flags[1] = value.trim() != "0",
                        _ => {}
                    }
                }
            }
            *cached = Some((Instant::now(), flags));
        }
        cached.as_ref().unwrap().1[index]
    })
}
