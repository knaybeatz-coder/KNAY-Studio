use lemon_chop::{Shared, editor::Logic, params::*, view};
use std::sync::Arc;
fn main() {
    let p = Arc::new(ChopParams::default());
    let shared = Arc::new(Shared::default());
    let mut l = Logic::new(p.clone(), shared.clone(), None);
    if let Some(path) = std::env::args().nth(2) {
        let audio = lemon_chop::audio::Audio::load(&path, &Default::default()).unwrap();
        let mut d = Document {
            path,
            ..Default::default()
        };
        d.autoset(1, audio.duration(), 120., 0., 1., &audio.onsets, 1);
        p.set_doc(d);
        *shared.audio.lock().unwrap() = Some(Arc::new(audio));
        shared.message("Ready — click pads to play; drag cue edges to trim");
        l.poll();
    }
    if std::env::var_os("CHOP_GRID").is_some() {
        l.button("Grid", Default::default());
        let mut d = (*p.document()).clone();
        d.markers = vec![0., 2., 4.1];
        p.set_doc(d);
    }
    let mut c = os_ui::Canvas::new(view::W, view::H, 1.).unwrap();
    l.paint(&mut c, &os_plugkit::palette());
    let mut pixels = vec![0; (view::W * view::H) as usize];
    c.copy_to_xrgb(&mut pixels);
    let mut image = tiny_skia::Pixmap::new(view::W, view::H).unwrap();
    for (p, v) in image.pixels_mut().iter_mut().zip(pixels) {
        *p = tiny_skia::PremultipliedColorU8::from_rgba(
            (v >> 16) as u8,
            (v >> 8) as u8,
            v as u8,
            255,
        )
        .unwrap();
    }
    image.save_png(std::env::args().nth(1).unwrap()).unwrap();
}
