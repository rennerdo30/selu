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
