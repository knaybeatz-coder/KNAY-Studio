// SPDX-License-Identifier: GPL-3.0-or-later
#[derive(Clone,Copy,Debug,PartialEq)]
pub(super) struct Page {pub left:f32,pub right:f32}
impl Page {pub(super) fn width(self)->f32{self.right-self.left}}
#[derive(Clone,Copy)]pub(super) enum Profile{Whole,Scheps,SslClassic,SslEv2,Tg12345}
pub(super) fn profile(d:&os_host::Descriptor)->Profile{
    if !d.vendor.eq_ignore_ascii_case("Waves"){return Profile::Whole;}
    if d.name.starts_with("Scheps Omni Channel 2"){Profile::Scheps}

    else if d.name.starts_with("Abbey Road EMI TG12345 Ch"){Profile::Tg12345}
    else if d.name.starts_with("SSL EV2 Channel"){Profile::SslEv2}
    else if d.name.starts_with("SSLChannel")||d.name.starts_with("SSLGChannel"){Profile::SslClassic}
    else{Profile::Whole}
}
pub(super) fn pages(profile:Profile,size:(u32,u32))->Vec<Page>{
    let ratio=size.0 as f32/size.1.max(1) as f32;
    // Geometry from the installed Waves V15 GUIXML. Unknown aspect/layout falls
    // back to the complete original editor, never equal-width guessed pages.
    let spec:Option<(f32,&[f32])>=match profile{
        Profile::Scheps if (1.26..=1.42).contains(&ratio)=>Some((840.,&[0.,115.,227.,339.,451.,601.,840.])),
        Profile::SslClassic if (0.49..=0.55).contains(&ratio)=>Some((357.,&[0.,180.,357.])),
        _=>None,
    };
    // Installed TG12345 has distinct mono/stereo layouts. Keep each complete
    // dynamics, EQ and output section between the dividers on its native faceplate.
    if matches!(profile,Profile::Tg12345){
        let spec=if (0.85..=0.92).contains(&ratio){Some((518.,[(0.,165.),(165.,331.),(331.,518.)]))}
            else if (1.08..=1.16).contains(&ratio){Some((658.,[(0.,205.),(205.,410.),(410.,658.)]))}else{None};
        if let Some((width,parts))=spec{return parts.into_iter().map(|(l,r)|Page{left:l*size.0 as f32/width,right:r*size.0 as f32/width}).collect();}
    }
    // Use the painted module boundaries; adjacent hit areas must not widen the displayed page.
    if matches!(profile,Profile::SslEv2)&&(0.72..=0.80).contains(&ratio){
        return [(0.,155.),(155.,310.),(310.,465.)].into_iter().map(|(l,r)|Page{left:l*size.0 as f32/465.,right:r*size.0 as f32/465.}).collect();
    }
    if let Some((width,cuts))=spec{cuts.windows(2).map(|p|Page{left:p[0]*size.0 as f32/width,right:p[1]*size.0 as f32/width}).collect()}
    else{vec![Page{left:0.,right:size.0 as f32}]}
}

#[cfg(test)] mod tests {
    #[test]fn section_pages_stop_at_complete_module_boundaries(){
        for factor in [0.5,1.,1.5,2.] {let p=super::pages(super::Profile::Scheps,((840.*factor) as u32,(614.*factor) as u32));assert_eq!(p.len(),6);for (page,pair) in p.iter().zip([0.,115.,227.,339.,451.,601.,840.].windows(2)){assert!((page.left-pair[0]*factor).abs()<0.01);assert!((page.right-pair[1]*factor).abs()<0.01);}assert_eq!(p.last().unwrap().right,840.*factor);}
        for (profile,size,cuts) in [(super::Profile::SslClassic,(357,675),vec![0.,180.,357.])]{let p=super::pages(profile,size);assert_eq!(p.len()+1,cuts.len());for (p,c) in p.iter().zip(cuts.windows(2)){assert_eq!((p.left,p.right),(c[0],c[1]));}}
        let ev=super::pages(super::Profile::SslEv2,(465,614));assert_eq!(ev.len(),3);
        assert_eq!(ev[0].right,155.);assert_eq!(ev[0].right,ev[1].left);assert_eq!(ev[1].right,ev[2].left);assert_eq!(ev[2].right,465.);
        assert_eq!(super::pages(super::Profile::Whole,(1200,600)).len(),1);
        assert_eq!(super::pages(super::Profile::Scheps,(1400,600)).len(),1,"Unknown expanded layout must not guess module boundaries");
        for (size,cuts) in [((518,587),[(0.,165.),(165.,331.),(331.,518.)]),((658,587),[(0.,205.),(205.,410.),(410.,658.)])]{
            for factor in [0.5,1.,1.5,2.] {let p=super::pages(super::Profile::Tg12345,((size.0 as f32*factor) as u32,(size.1 as f32*factor) as u32));assert_eq!(p.len(),3);for (p,(l,r)) in p.iter().zip(cuts){assert!((p.left-l*factor).abs()<0.01);assert!((p.right-r*factor).abs()<0.01);}}
        }
        assert_eq!(super::pages(super::Profile::Tg12345,(300,587)).len(),1);

    }
}

/// Both sizes are physical pixels, including the display's backing scale.
#[cfg(any(target_os="macos",test))]
pub(super) fn mac_strip_zoom(height:u32,available:i32)->f64 {
    available.max(1) as f64 / height.max(1) as f64
}
#[cfg(test)] mod mac_fit_tests {
    use super::*;
    #[test] fn mac_retina_fit_matches_windows_height_and_section_width() {
        for backing in [1.,2.] {for ui in [1.,1.5,2.] {for available in [300,500,900] {
            let height=(675.*backing) as u32;
            let zoom=mac_strip_zoom(height,available);
            assert!((height as f64*zoom-available as f64).abs()<0.001);
            let windows_fit=available as f64/675.;
            let mac_page_width=180.*backing*zoom/ui as f64;
            assert!((mac_page_width-180.*windows_fit/ui as f64).abs()<0.001);
            // Cocoa bounds transforms must also invert mouse coordinates.
            let native_y=217.;let displayed_y=native_y*zoom;
            assert!((displayed_y/zoom-native_y).abs()<0.001);
        }}}
    }
    #[test] fn very_short_and_tall_views_keep_the_whole_strip_in_frame() {
        for available in [10, 10000] {
            assert!((1350. * mac_strip_zoom(1350,available) - available as f64).abs() < 1e-9);
        }
    }
}
