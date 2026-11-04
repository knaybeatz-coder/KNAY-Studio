//! Offline reconstruction comparison, no audio device.
use lemon_wave::{
    reference_envelope::{Envelope, Settings},
    reference_filter::{Coefficients, Filter},
    reference_oscillator::{Decimator, Oscillator, Table, Warp},
};
use std::{path::Path, sync::Arc};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let root = Path::new(&args[1]);
    let scale = args
        .get(2)
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(std::f32::consts::SQRT_2);
    let cutoff_multiplier = args
        .get(4)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.);
    let rate_factor = args
        .get(3)
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(2.);
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("cases.json")).unwrap()).unwrap();
    let table = Arc::new(Table::init_saw());
    std::fs::create_dir_all(root.join("reconstructed")).unwrap();
    for case in cases.as_array().unwrap() {
        let rate = case["sample_rate"].as_u64().unwrap() as f32;
        let label = case["label"].as_str().unwrap();
        let values = case["parameters"].as_array().unwrap();
        let get = |id: u64, default: f64| {
            values
                .iter()
                .find(|v| v[0].as_u64() == Some(id))
                .and_then(|v| v[1].as_f64())
                .unwrap_or(default)
        };
        let settings = Settings {
            attack: 32. * get(3000000, 0.).powi(5),
            hold: 32. * get(3000001, 0.).powi(5),
            decay: 32. * get(3000002, 0.).powi(5),
            sustain: get(3000003, 1.).powi(2) as f32,
            release: 32. * get(3000004, 0.).powi(5),
            curves: [get(3000005, 0.5), get(3000006, 0.5), get(3000007, 0.5)],
        };
        let typ = (get(2000002, 0.) * 107.).round() as usize;
        let mode = match typ {
            5 | 7 => 1,
            9 | 11 => 3,
            12 | 13 => 2,
            _ => 0,
        };
        if label.starts_with("filter") && mode == 0 {
            continue;
        }
        let poles = if [7, 11, 13].contains(&typ) { 4 } else { 2 };
        let cutoff = 8.175798915643707 * (22050_f64 / 8.175798915643707).powf(get(2000003, 0.5));
        let coeff = Coefficients::new(
            rate * rate_factor,
            cutoff * cutoff_multiplier,
            get(2000004, 0.1),
            get(2000005, 0.) as f32,
            mode,
            poles,
        );
        let mut filter = Filter::default();
        let mut env = Envelope::default();
        env.start(&settings, rate * 2., false);
        let mut oscillator = Oscillator::new(table.clone(), rate);
        let mut decimator = Decimator::default();
        let mut bytes = Vec::new();
        for i in 0..case["frames"].as_u64().unwrap() {
            if case["off"].as_u64() == Some(i) {
                env.off(&settings, rate * 2.);
            }
            let pair = oscillator.tick_pair(440., Warp::Off, 0., [0.75 * 0.75; 2]);
            let pair = pair.map(|x| {
                let y = if mode == 0 {
                    x * scale
                } else {
                    filter.tick(x * scale, &coeff)
                };
                y * env.tick(&settings, rate * 2.)
            });
            let y = decimator.tick(pair) * 0.0625 * std::f32::consts::SQRT_2 / scale;
            bytes.extend(y.to_le_bytes());
        }
        std::fs::write(
            root.join("reconstructed").join(format!("{label}.f32")),
            bytes,
        )
        .unwrap();
    }
}
