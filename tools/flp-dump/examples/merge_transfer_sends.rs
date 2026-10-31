fn main() -> Result<(),Box<dyn std::error::Error>> {
 let args:Vec<String>=std::env::args().collect();
 let read=|p:&str|->Result<os_model::Project,Box<dyn std::error::Error>> {Ok(os_flp::project::load(&os_flp::FlpFile::parse(&std::fs::read(p)?)?)?)};
 let mut original=read(&args[1])?;let prepared=read(&args[2])?;let baseline=original.clone();
 let labels=["VERB LONG","VERB SHORT","VOCAL VERB","ANALOG VERB","SLAP","1/8","1/4","1/2","DELAY VERB"];
 for label in labels {let source=prepared.mixer.iter().find(|m|m.name.as_deref()==Some(label)).unwrap();let target=original.mixer.iter_mut().find(|m|m.name.as_deref()==Some(label)).unwrap();target.slots[0]=source.slots[0].clone();target.slots_enabled=true;target.slot_on[0]=true;target.slot_mix[0]=1.0;}
 let mut file=os_flp::FlpFile::parse(&std::fs::read(&args[1])?)?;os_flp::project::apply(&original,&mut file)?;let bytes=file.to_bytes()?;let back=os_flp::project::load(&os_flp::FlpFile::parse(&bytes)?)?;
 for old in &baseline.mixer {let new=back.mixer.iter().find(|m|m.index==old.index).unwrap();assert_eq!(new.name,old.name);assert_eq!(new.routes,old.routes);assert_eq!(new.send_levels,old.send_levels);assert_eq!(new.color,old.color);if !labels.contains(&old.name.as_deref().unwrap_or("")) {assert_eq!(new.slots,old.slots);}}
 for (new,old) in back.channels.iter().zip(&baseline.channels) {assert_eq!(new.plugin,old.plugin);assert_eq!(new.plugin_state,old.plugin_state);assert_eq!(new.sample_path,old.sample_path);assert_eq!(new.mixer_track,old.mixer_track);assert_eq!(new.name,old.name);assert_eq!(new.kind,old.kind);assert!((new.volume-old.volume).abs()<0.0001);}
 std::fs::write(&args[2],bytes)?;
 println!("Preserved original channels, routing, send levels and all other effect slots; updated nine sends.");Ok(())
}
