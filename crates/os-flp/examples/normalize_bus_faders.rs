use std::path::PathBuf;
fn main() {
    let mut args=std::env::args().skip(1);
    let input=PathBuf::from(args.next().expect("input FLP"));
    let output=args.next().map(PathBuf::from);
    let flags:Vec<_>=args.collect();
    let all=flags.iter().any(|v|v=="--all");
    let effects_off=flags.iter().any(|v|v=="--effects-off");
    let mut file=os_flp::FlpFile::parse(&std::fs::read(&input).unwrap()).unwrap();
    let mut project=os_flp::project::load(&file).unwrap();
    let mut buses=Vec::new();
    for m in &mut project.mixer {
        if effects_off {m.slots_enabled=m.is_fx_return();}
        let name=m.name.as_deref().unwrap_or("");
        let lower=name.to_lowercase();
        let bus=lower.contains("mix") || lower=="effects" || lower=="fx" || lower=="master";
        if all || bus {println!("{} {name}: {:?} -> unity (0.8)",m.index,m.volume);m.volume=Some(0.8);buses.push(m.index);}
    }
    for c in &mut project.init_controls {
        if let os_model::target::Target::MixerVolume(index)=os_model::target::decode(c.target) {
            if buses.contains(&index) {c.value=12800;}
        }
    }
    if let Some(output)=output {
        os_flp::project::apply(&project,&mut file).unwrap();
        let bytes=file.to_bytes().unwrap();
        let back=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
        assert_eq!(back,project);
        std::fs::write(output,bytes).unwrap();
    }
}
