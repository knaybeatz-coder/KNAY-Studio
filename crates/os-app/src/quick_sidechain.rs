//! One-click, non-destructive setup of a keyed Lemon Limiter at the end of an insert chain.
use crate::{app::App, mixer as mx};
use lemon_limiter::params as lp;
use os_engine::effects::dynamics as dsp;

fn preset(port: usize) -> Vec<u8> {
    let mut values = dsp::DEFAULTS;
    for (k, text) in [
        (dsp::COMP_THRES, "-30"),
        (dsp::COMP_RATIO, "4:1"),
        (dsp::COMP_REL, "200"),
    ] {
        values[k] = lp::parse_text(lp::DEFS[k].text, text, dsp::RANGES[k].0, dsp::RANGES[k].1).unwrap();
    }
    let mut raw = lp::to_fl(&values, &lp::default_state());
    raw[lp::PAGE_BYTE] = 1;
    raw[lp::SIDECHAIN_BYTE..lp::SIDECHAIN_BYTE + 4].copy_from_slice(&(port as i32).to_le_bytes());
    lp::plugin_state_from_fl(&raw)
}
pub fn run(app: &mut App, dst: u16) -> Option<String> {
    let src = app.state.selected_mixer;
    if src == 0 || src == dst || mx::track(&app.state, dst).is_none() {
        return Some("Select the trigger track, then Quick Sidechain on a different destination".into());
    }
    if mx::makes_loop(&app.state, src, dst) {
        return Some("Quick Sidechain would create a routing loop; nothing changed".into());
    }
    let m = mx::track(&app.state, dst)?;
    let capacity = mx::user_slot_capacity(&app.state,dst);
    let logical = (0..capacity)
        .rfind(|i| m.slots[mx::visible_user_slot(&app.state,dst,*i) as usize].is_some())
        .map_or(9, |i| (i + 1).max(9));
    if logical >= capacity {
        return Some("This channel has reached the 63-effect limit".into());
    }
    let slot = mx::visible_user_slot(&app.state,dst,logical);
    let Some(index) = app.plugins.effect_index("Lemon Limiter") else {
        return Some("Lemon Limiter is not available yet; try again after plugin loading finishes".into());
    };
    let old_inputs = mx::inputs(&app.state, dst);
    let mut inputs = old_inputs.clone();
    if !inputs.contains(&src) {
        inputs.push(src);
        inputs.sort_unstable();
    }
    let port = inputs.iter().position(|t| *t == src).unwrap() + 1;
    let state = preset(port);
    let reference = crate::project_plugins::plugin_ref(&app.plugins.effects[index].desc, state.clone());
    let wrapper = os_flp::wrapper::encode(None, &reference);
    let Some(wrapper) = os_flp::wrapper::set_input_port(&wrapper, 1, port as i32) else {
        return Some("Could not configure the sidechain input".into());
    };
    // Capture live settings before recording an undo step for the two changed mixer channels.
    app.store_builtin_states();
    if let Err(e)=crate::project_plugins::store_hosted(&mut app.state.project, &app.plugins){return Some(e);}
    let before = crate::history::Snap::Several(
        [src, dst]
            .into_iter()
            .filter_map(|t| mx::channel_copy(&app.state, t).map(|c| crate::history::Snap::MixerChannel(Box::new(c))))
            .collect(),
    );
    if let Err(e) = app.plugins.create_effect(dst, slot, index, Some(&state)) {
        return Some(format!("Quick Sidechain: {e}"));
    }
    crate::history::record(&mut app.state, "Quick Sidechain", before);
    let m = mx::track_mut(&mut app.state, src)?;
    if !m.routes.contains(&dst) {
        m.routes.push(dst);
        m.routes.sort_unstable();
    }
    m.send_levels.retain(|(t, _)| *t != dst);
    m.send_levels.push((dst, 0.));
    let m = mx::track_mut(&mut app.state, dst)?;
    for effect in m.slots.iter_mut().flatten() {
        if effect.name == os_flp::wrapper::WRAPPER {
            for (bus, old) in os_flp::wrapper::input_ports(&effect.state).iter().enumerate() {
                if *old > 0 {
                    if let Some(source) = old_inputs.get(*old as usize - 1) {
                        if let Some(new) = inputs.iter().position(|t| t == source) {
                            if let Some(state) = os_flp::wrapper::set_input_port(&effect.state, bus, (new + 1) as i32) {
                                effect.state = state;
                            }
                        }
                    }
                }
            }
        } else if effect.name == "Fruity Limiter" && effect.state.len() >= 168 {
            let old = i32::from_le_bytes(effect.state[164..168].try_into().unwrap());
            if old > 0 {
                if let Some(source) = old_inputs.get(old as usize - 1) {
                    if let Some(new) = inputs.iter().position(|t| t == source) {
                        effect.state[164..168].copy_from_slice(&((new + 1) as i32).to_le_bytes());
                    }
                }
            }
        }
    }
    m.slots[slot as usize] = Some(os_model::SlotPlugin {
        name: os_flp::wrapper::WRAPPER.into(),
        state: wrapper,
    });
    m.slot_on[slot as usize] = true;
    m.slot_mix[slot as usize] = 1.;
    m.slots_enabled = true;
    app.plugins.added(crate::plugins::fx_key(dst, slot));
    app.transport.set_effects(app.plugins.effects());
    app.state.piano.dirty = true;
    app.state.mixer.slot_scroll.insert(dst, logical.saturating_sub(8));
    app.invalidate();
    Some(format!(
        "Quick Sidechain: {} → {}, slot {}. COMP, −30 dB, 4:1, 200 ms release{}",
        mx::name(&app.state, src),
        mx::name(&app.state, dst),
        logical + 1,
        if app.state.mixer.all_fx_bypassed {
            ". All FX bypass is still on; switch it off to hear ducking"
        } else {
            ""
        }
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_sidechain_appends_configures_and_saves() {
        let mut a = App::new(None);
        a.state.project = crate::app::test_project();
        a.plugins.effects = crate::plugins::builtin_effects();
        a.state.selected_mixer = 3;
        crate::playlist::arr_mut(&mut a.state);
        let source_routes = a.state.project.mixer[3].routes.clone();
        a.state.project.mixer[1].routes.push(2); // Already input 1; trigger must become input 2.
        let original = os_model::SlotPlugin {
            name: "Keep me".into(),
            state: vec![1, 2, 3],
        };
        a.state.project.mixer[2].slots[9] = Some(original.clone());
        let eq = os_model::SlotPlugin {
            name: "Fruity Parametric EQ 2".into(),
            state: lemon_parametric_eq_2::params::default_state(),
        };
        a.state.project.mixer[2].slots[10] = Some(eq.clone());
        assert!(run(&mut a, 2).unwrap().starts_with("Quick Sidechain:"));
        let m = &a.state.project.mixer[2];
        assert_eq!(m.slots[9], Some(original.clone()));
        assert_eq!(m.slots[10], Some(eq.clone()));
        assert!(m.slot_on[11] && m.slots_enabled);
        assert_eq!(mx::slot_count(&a.state, 2), 12);
        let reference = crate::project_plugins::wrapped_slot(m.slots[11].as_ref().unwrap()).unwrap();
        let raw = lp::fl_from_plugin_state(&reference.state).unwrap();
        let values = lp::values_of(&raw);
        assert!(lp::value_text(lp::DEFS[dsp::COMP_THRES].text, values[dsp::COMP_THRES]).starts_with("-30.0dB"));
        assert_eq!(
            lp::value_text(lp::DEFS[dsp::COMP_RATIO].text, values[dsp::COMP_RATIO]),
            "4.0:1"
        );
        assert_eq!((raw[lp::PAGE_BYTE], raw[lp::SIDECHAIN_BYTE]), (1, 2));
        assert_eq!(crate::project_plugins::slot_ports(&a.state.project, 2, 11), (0, 2));
        assert!(!a.plugins.editor_open());
        assert!(a.plugins.hosted.contains_key(&crate::plugins::fx_key(2, 11)));
        for route in source_routes {
            assert!(a.state.project.mixer[3].routes.contains(&route));
        }
        assert!(a.state.project.mixer[3].send_levels.contains(&(2, 0.)));
        let file = os_flp::project::create(&a.state.project).unwrap();
        let restored = os_flp::project::load(&os_flp::FlpFile::parse(&file.to_bytes().unwrap()).unwrap()).unwrap();
        assert_eq!(restored.mixer[2].slots[11], a.state.project.mixer[2].slots[11]);
        assert_eq!(crate::project_plugins::slot_ports(&restored, 2, 11), (0, 2));
        assert_eq!(
            os_model::target::decode(os_model::target::slot_param(2, 11, 9)),
            os_model::target::Target::SlotParam(2, 11, 9)
        );
        crate::history::undo(&mut a.state);
        assert!(a.state.project.mixer[2].slots[11].is_none());
        assert_eq!(a.state.project.mixer[2].slots[9], Some(original));
        crate::history::redo(&mut a.state);
        assert!(a.state.project.mixer[2].slots[11].is_some());
        let before = a.state.project.clone();
        a.state.selected_mixer = 2;
        assert!(run(&mut a, 3).unwrap().contains("loop"));
        assert_eq!(a.state.project, before);
        // The exact preset must react to an external key, not the instrument's own signal.
        let vals: [i32; dsp::PARAMS] = values.try_into().unwrap();
        let render = |key: f32| {
            let mut d = dsp::Dynamics::new(48000., vals, 0);
            let mut l = vec![0.2; 24000];
            let mut r = l.clone();
            let k = vec![key; 24000];
            d.process(&mut l, &mut r, Some((&k, &k)));
            l[23000].abs()
        };
        let quiet = render(0.);
        let ducked = render(0.8);
        assert!(quiet > 0.19 && ducked < quiet * 0.25, "{quiet} → {ducked}");
    }
}
