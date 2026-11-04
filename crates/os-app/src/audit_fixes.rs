use crate::app::App;
#[test]fn audit_plugin_edits_backup_and_failed_save(){
 let dir=std::env::temp_dir().join(format!("audit-plugin-save-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
 crate::backup::TEST_DIR.with(|d|*d.borrow_mut()=Some(dir.join("Backup")));
 let mut app=App::new(None);app.plugins=crate::plugins::Plugins::builtins();
 let row=crate::app::add_plugin_channel(&mut app.state.project,"Lemon Wave");let i=app.plugins.index_of("Lemon Wave").unwrap();app.plugins.create(row,i).unwrap();app.state.hosted_rows.push(row);
 let file=dir.join("song.lmn");app.save_to(file.clone());assert!(file.exists(),"{:?}",app.state.message);assert!(!app.unsaved());
 app.plugins.mark_edited(row);assert!(app.unsaved());assert!(app.changed_since_backup());
 let at=chrono::NaiveDate::from_ymd_opt(2026,11,3).unwrap().and_hms_opt(12,0,0).unwrap();assert!(app.write_backup(at).unwrap().is_ok());assert!(app.unsaved());assert!(!app.changed_since_backup());app.plugins.mark_edited(row);assert!(app.changed_since_backup());
 let disk=std::fs::read(&file).unwrap();let model=app.state.project.clone();app.plugins.state_fail.insert(row);assert!(crate::project_plugins::store_hosted(&mut app.state.project,&app.plugins).is_err());assert_eq!(model,app.state.project);
 app.save_to(file.clone());assert!(app.state.message.as_ref().unwrap().starts_with("Save failed:"));assert_eq!(std::fs::read(&file).unwrap(),disk);assert!(app.unsaved());
 app.plugins.state_fail.clear();app.save_to(file.clone());assert!(!app.unsaved());
 let invalid=dir.join("not-a-directory");std::fs::write(&invalid,b"file").unwrap();assert!(crate::backup::keep_overwritten(&file,Some(invalid)).is_err());assert!(file.exists());
 crate::backup::TEST_DIR.with(|d|*d.borrow_mut()=None);std::fs::remove_dir_all(dir).unwrap();
}
