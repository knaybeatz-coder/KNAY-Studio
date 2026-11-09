//! Synthetic, offscreen analyzer-and-drag benchmark. No device, microphone or GUI automation.
use lemon_parametric_eq_2::{
    analyzer::{LiveAnalyzer, Look as AnalyzerLook},
    editor::{Btn, Logic, Mods},
    params::{self, EqParams, Opt},
    view::{self, Look},
    Shared,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&out).unwrap();
    for (w, h, dpi) in [(1000, 620, 1.), (640, 420, 1.5), (1000, 620, 2.)] {
        let shared = Arc::new(Shared::default());
        let mut logic = Logic::new(Arc::new(EqParams::default()), shared.clone(), None);
        logic.view.size = (w as f32, h as f32);
        logic.view.selected = 3;
        logic.view.selection = 8;
        logic.view.show_dynamics = true;
        logic.vals.edit_plain(&params::dynamic_id(12), -6.);
        let mut c =
            os_ui::Canvas::new((w as f32 * dpi) as u32, (h as f32 * dpi) as u32, dpi).unwrap();
        let mut pixels = vec![0u32; (c.pixmap.width() * c.pixmap.height()) as usize];
        let mut live = LiveAnalyzer::default();
        let mut samples = 0;
        let mut timings = Vec::new();
        let mut seed = 12345u32;
        for frame in 0..180 {
            {
                let mut scope = shared.scope.lock().unwrap();
                for _ in 0..735 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let noise = (seed >> 8) as f32 / 16777216. - 0.5;
                    let x = 0.15 * (samples as f32 * std::f32::consts::TAU * 220. / 44100.).sin()
                        + 0.02 * noise;
                    let pos = scope.pos;
                    scope.buf[pos] = x;
                    scope.pos = (pos + 1) % scope.buf.len();
                    scope.written += 1;
                    samples += 1;
                }
            }
            let start = Instant::now();
            let extra = logic.shown_extra();
            let alook = AnalyzerLook {
                precision: params::opt(&extra, Opt::FreqPrecision) as usize,
                slope: 3,
                smoothing: 3,
                average: false,
            };
            live.poll_at(
                &shared,
                Some(alook),
                Duration::from_secs_f64(frame as f64 / 60.),
            );
            if frame == 30 {
                let (x, y) = logic.layout().node(&logic.shown(), 3);
                logic.down(x, y, Btn::Left, Mods::default(), false);
            }
            if (31..120).contains(&frame) {
                let l = logic.layout();
                logic.moved(
                    l.x(0.5 + (frame as f64 * 0.05).sin() * 0.1),
                    l.y((frame as f64 * 0.08).sin() * 4.),
                    Mods::default(),
                );
            }
            if frame == 120 {
                logic.up(Btn::Left);
            }
            logic.view.reduction[3] = -3. - 2. * (frame as f32 * 0.05).sin();
            let extra = logic.shown_extra();
            logic.hits = view::paint(
                &mut c,
                &Look {
                    s: logic.shown(),
                    extra: &extra,
                    spectrum: Some(&live.analyzer.cols),
                    rate: 44100.,
                },
                &logic.view,
            );
            c.copy_to_xrgb(&mut pixels);
            timings.push(start.elapsed().as_secs_f64() * 1000.);
            if frame == 150 {
                c.pixmap
                    .save_png(std::path::Path::new(&out).join(format!("analyzer-{w}-{dpi}.png")))
                    .unwrap();
            }
        }
        timings.sort_by(f64::total_cmp);
        println!("{w}x{h} dpi{dpi}: 180 animated frames, {} FFTs, median {:.3}ms p95 {:.3}ms p99 {:.3}ms max {:.3}ms",live.measurements,timings[90],timings[171],timings[178],timings[179]);
    }
}
