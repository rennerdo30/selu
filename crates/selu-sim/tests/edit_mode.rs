use selu_sim::{
    Session, SessionConfig,
    world::{EditModePreset, Scenario},
};
#[test]
fn preset_round_trips_and_survives_reload_without_edit_mode() {
    assert_eq!(
        toml::from_str::<Scenario>("").unwrap().edit_mode_preset,
        EditModePreset::Modern
    );
    let mut s = Session::new(Default::default()).unwrap();
    assert_eq!(s.sim().edit_mode_preset().unwrap(), EditModePreset::Modern);
    let root = std::env::temp_dir().join(format!("selu-no-edit-mode-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Interface")).unwrap();
    std::fs::write(root.join("Interface/ui-toc-list.txt"), "").unwrap();
    s.load_blizzard_ui(&root, None, |_| {}).unwrap();
    let mut state = Scenario::capture(s.sim()).unwrap();
    state.edit_mode_preset = EditModePreset::Classic;
    let text = toml::to_string(&state).unwrap();
    let restored: Scenario = toml::from_str(&text).unwrap();
    assert_eq!(restored, state);
    state.apply(s.sim()).unwrap();
    state.apply(s.sim()).unwrap();
    assert!(!s.sim().edit_mode_available());
    s.eval("assert(C_EditMode.GetLayouts().activeLayout==1)")
        .unwrap();
    assert_eq!(
        s.logs()
            .iter()
            .filter(|l| l.contains("Edit Mode preset ignored"))
            .count(),
        1
    );
    s.reload().unwrap();
    assert_eq!(
        Scenario::capture(s.sim()).unwrap().edit_mode_preset,
        EditModePreset::Classic
    );
}
#[test]
#[ignore = "requires SELU_CLIENT_INSTALL and optional SELU_DEFAULT_UI_SOURCE"]
fn client_presets_apply_source_geometry_through_native_state() {
    let install = std::path::PathBuf::from(std::env::var_os("SELU_CLIENT_INSTALL").unwrap());
    let source = std::env::var_os("SELU_DEFAULT_UI_SOURCE")
        .map(std::path::PathBuf::from)
        .unwrap_or(install.clone());
    let mut s = Session::new(SessionConfig {
        target: Some("wowf".into()),
        client_install: Some(install),
        ..Default::default()
    })
    .unwrap();
    s.load_blizzard_ui(&source, None, |_| {}).unwrap();
    s.login().unwrap();
    for preset in [EditModePreset::Modern, EditModePreset::Classic] {
        if preset == EditModePreset::Classic {
            let mut state = Scenario::capture(s.sim()).unwrap();
            state.edit_mode_preset = preset;
            state.apply(s.sim()).unwrap();
            s.reload().unwrap();
        }
        for _ in 0..4 {
            s.advance_time(1. / 60.);
        }
        let map = if preset == EditModePreset::Modern {
            "EDIT_MODE_MODERN_SYSTEM_MAP"
        } else {
            "EDIT_MODE_CLASSIC_SYSTEM_MAP"
        };
        let name = if preset == EditModePreset::Modern {
            "Modern"
        } else {
            "Classic"
        };
        s.eval(&format!(r#"
            local layout=EditModeManagerFrame:GetActiveLayoutInfo()
            assert(layout.layoutIndex==Enum.EditModePresetLayouts.{name})
            assert(C_EditMode.GetEditModeDefaultLayout()==Enum.EditModePresetLayouts.{name})
            local expected={map}[PlayerFrame.system][PlayerFrame.systemIndex].anchorInfo
            local point,relative,rpoint,x,y=PlayerFrame:GetPoint(1)
            assert(point==expected.point and relative==_G[expected.relativeTo] and rpoint==expected.relativePoint)
            assert(x==expected.offsetX and y==expected.offsetY)
        "#)).unwrap();
        s.snapshot().unwrap();
        s.eval(if preset==EditModePreset::Modern {
            "assert(PlayerFrame:GetBottom()>0 and PlayerFrame:GetTop()<UIParent:GetHeight()/2)"
        }else{
            "assert(PlayerFrame:GetLeft()>=0 and PlayerFrame:GetLeft()<20);assert(math.abs(PlayerFrame:GetTop()-UIParent:GetTop())<20)"
        }).unwrap();
    }
}
