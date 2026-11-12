//! Blizzard chunk returns are addon-scoped modules, not discarded values.
#[test]
fn relative_modules_keep_their_addon_scope_after_loading() {
    let root = std::env::temp_dir().join(format!("selu-ui-modules-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let mut list = String::new();
    for (name, value) in [("ModuleOne", 1), ("ModuleTwo", 2)] {
        let dir = root.join("Interface/AddOns").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.toc")), "Mod.lua\nConsumer.lua\n").unwrap();
        std::fs::write(dir.join("Mod.lua"), format!("return {{value={value}}}")).unwrap();
        std::fs::write(dir.join("Consumer.lua"), format!("function {name}() return import('.Mod').value end; function {name}Shared() return import('ModuleOne.Mod').value end")).unwrap();
        list.push_str(&format!("Interface/AddOns/{name}/{name}.toc\n"));
    }
    std::fs::write(root.join("Interface/ui-toc-list.txt"), list).unwrap();
    let mut s = selu_sim::Session::new(Default::default()).unwrap();
    let report = s.load_default_ui(&root, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    s.eval("assert(ModuleOne()==1 and ModuleTwo()==2);assert(ModuleOneShared()==1 and ModuleTwoShared()==1)").unwrap();
}

/// `C_AddOns.LoadAddOn` of a load-on-demand Blizzard addon runs it from the
/// default UI's source, after the dependencies that are not loaded yet.
#[test]
fn load_on_demand_blizzard_addons_load_from_the_default_ui_source() {
    let root = std::env::temp_dir().join(format!("selu-ui-lod-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    let addons = [
        (
            "Base",
            "",
            "LoadOrder = {}; local f = CreateFrame('Frame'); f:RegisterEvent('ADDON_LOADED'); f:SetScript('OnEvent', function(_, _, name) LoadOrder[#LoadOrder + 1] = name end)",
        ),
        (
            "LazyBase",
            "## LoadOnDemand: 1\n",
            "LazyBaseRuns = (LazyBaseRuns or 0) + 1",
        ),
        (
            "LazyTemplates",
            "## LoadOnDemand: 1\n## Dependencies: LazyBase\n",
            "LazyRuns = (LazyRuns or 0) + 1; LazySeesBase = LazyBaseRuns == 1",
        ),
    ];
    let mut list = String::new();
    for (name, header, lua) in addons {
        let dir = source.join("Interface/AddOns").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{name}.toc")),
            format!("{header}{name}.lua\n"),
        )
        .unwrap();
        std::fs::write(dir.join(format!("{name}.lua")), lua).unwrap();
        list.push_str(&format!("Interface/AddOns/{name}/{name}.toc\n"));
    }
    std::fs::write(source.join("Interface/ui-toc-list.txt"), list).unwrap();
    let user = root.join("AddOns/Main");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("Main.toc"), "main.lua\n").unwrap();
    std::fs::write(user.join("main.lua"), "").unwrap();
    let mut s = selu_sim::Session::new(Default::default()).unwrap();
    let report = s.load_default_ui(&source, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    s.load_addon_dir(&user).unwrap();
    s.eval(
        "assert(LazyRuns == nil and not C_AddOns.IsAddOnLoaded('LazyTemplates'))
         assert(C_AddOns.LoadAddOn('LazyTemplates') == true)
         assert(LazyRuns == 1 and LazyBaseRuns == 1 and LazySeesBase)
         assert(C_AddOns.IsAddOnLoaded('LazyTemplates') and C_AddOns.IsAddOnLoaded('LazyBase'))
         local n = #LoadOrder
         assert(LoadOrder[n - 1] == 'LazyBase' and LoadOrder[n] == 'LazyTemplates', table.concat(LoadOrder, ','))
         -- A second load runs nothing again.
         assert(C_AddOns.LoadAddOn('LazyTemplates') == true and LazyRuns == 1)
         local loaded, reason = C_AddOns.LoadAddOn('NoSuchAddon')
         assert(loaded == nil and reason == 'MISSING', tostring(reason))",
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
}
