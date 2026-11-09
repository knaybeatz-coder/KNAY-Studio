//! Profile visibility for the compact EQ band card.
pub const BAND_CARD: &str = "visible_eq-band-card";

pub fn band_card() -> bool {
    use std::{cell::Cell, time::{Duration, Instant}};
    thread_local! {
        static CACHED: Cell<Option<(Instant, bool)>> = const { Cell::new(None) };
    }
    CACHED.with(|cache| {
        if let Some((at, shown)) = cache.get() {
            if at.elapsed() < Duration::from_millis(300) {
                return shown;
            }
        }
        let hidden = os_paths::user_file("settings.txt")
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| text.lines().rev().filter_map(|line| line.split_once('='))
                .find(|(key, _)| key.trim() == BAND_CARD).map(|(_, value)| value.trim() == "0"))
            .unwrap_or(false);
        cache.set(Some((Instant::now(), !hidden)));
        !hidden
    })
}
