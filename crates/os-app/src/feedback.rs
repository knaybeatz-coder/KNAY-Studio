//! Explicit user-initiated report form. Diagnostics stay in the URL fragment until reviewed and submitted.
pub const URL:&str="https://michaellanemusic.de/lemon-studio/features/feedback.html";
pub fn address()->String {
    let a=crate::audio_cfg::AudioCfg::current();
    let text=format!("Lemon Studio build: {}\nOS: {} ({})\nConfigured audio backend: {}\nConfigured device: {}\nConfigured sample rate: {}\nConfigured buffer: {}\nMode: {}\nOS version (please add): \nAffected plugin and version (if relevant): ",crate::updates::VERSION,std::env::consts::OS,std::env::consts::ARCH,if a.asio{"ASIO"}else{std::env::consts::OS},a.device.as_deref().unwrap_or("System default"),a.rate.map_or("Device default".into(),|n|n.to_string()),a.buffer.map_or("Device default".into(),|n|if n==0{"Smallest supported".into()}else{n.to_string()}),if a.exclusive{"Exclusive"}else{"Shared / driver default"});
    let encoded:String=text.bytes().map(|b|format!("%{b:02X}")).collect();
    format!("{URL}#environment={encoded}")
}
pub fn open()->Result<(),String>{crate::donate::open_url(&address())}
#[cfg(test)]mod tests{
    #[test]fn feedback_diagnostics_are_reviewable_fragment_and_button_is_visible(){
        let address=super::address();assert!(address.starts_with(super::URL));assert!(!address.contains('?'));assert!(address.contains("#environment="));
        let mut app=crate::app::App::new(None);
        for width in [1024,1280,1920]{let mut canvas=os_ui::Canvas::new(width,900,1.).unwrap();app.paint(&mut canvas,None);let button=app.hits.iter().find(|h|h.action==crate::app::Action::FeatureVote(0,30)).expect("Visible report button");assert!(button.rect.x>=0.&&button.rect.right()<width as f32);for h in &app.hits{if matches!(h.action,crate::app::Action::Updates|crate::app::Action::Donate){assert!(button.rect.x>=h.rect.right()||button.rect.right()<=h.rect.x);}}}
    }
}
