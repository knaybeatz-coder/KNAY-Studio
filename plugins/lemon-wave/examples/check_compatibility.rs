//! Compare an old CLAP patch/render with the updated instrument; offline only.
use os_host::Library;
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let root = Path::new(&args[3]);
    let lib = os_clap::Library::load(Path::new(&args[2])).unwrap();
    let id = lib.plugins()[0].id.clone();
    let instance = lib.create(&id).unwrap();
    let mut p = instance.lock().unwrap();
    assert!(p.activate(48000));
    if args[1] == "capture" && args.len() > 4 {
        let bytes = std::fs::read(&args[4]).unwrap();
        let body = bytes.splitn(3, |b| *b == b'\n').nth(2).unwrap();
        assert!(p.load_state(body));
        p.idle();
    }
    if args[1] == "compare" {
        let source = (0..p.param_count())
            .filter_map(|i| p.param_info(i))
            .find(|i| i.name == "A Source")
            .unwrap()
            .id;
        assert!(p.set_param(source, 1.));
        for setting in &lemon_wave::controls::SETTINGS[50..] {
            let id = (0..p.param_count())
                .filter_map(|i| p.param_info(i))
                .find(|i| i.name == setting.name)
                .unwrap()
                .id;
            assert!(p.set_param(id, 0.91));
        }
        let mut l = [0.; 256];
        let mut r = l;
        p.process(48000, &mut l, &mut r);
        assert!(p.load_state(&std::fs::read(root.join("old.state")).unwrap()));
        p.idle();
        assert_eq!(
            p.param_value(source),
            Some(
                std::fs::read_to_string(root.join("source.txt"))
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.)
            ),
            "old state must restore its oscillator source"
        );
    } else {
        std::fs::create_dir_all(root).unwrap();
        let source = (0..p.param_count())
            .filter_map(|i| p.param_info(i))
            .find(|i| i.name == "A Source")
            .map(|i| p.param_value(i.id).unwrap())
            .unwrap_or(0.);
        std::fs::write(root.join("source.txt"), source.to_string()).unwrap();
        std::fs::write(root.join("old.state"), p.save_state().unwrap()).unwrap();
    }
    p.reset();
    p.note_on(0, 60, 0.8);
    let mut l = vec![0.; 8192];
    let mut r = l.clone();
    p.process(48000, &mut l, &mut r);
    let bytes: Vec<_> = l.iter().chain(&r).flat_map(|x| x.to_le_bytes()).collect();
    if args[1] == "compare" {
        assert_eq!(
            bytes,
            std::fs::read(root.join("old.f32")).unwrap(),
            "legacy sound changed"
        );
        println!("Old patch restores its original source and renders bit-identically");
    } else {
        std::fs::write(root.join("old.f32"), bytes).unwrap();
        println!("Captured old installed patch and synthetic render");
    }
}
