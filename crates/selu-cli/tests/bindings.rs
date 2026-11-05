mod common;
#[test]
fn labels_and_callback_are_readable_and_callable() {
    let lua = common::compile(
        r#"keybinding("Demo_TOGGLE", L`Toggle panel`, () => { print("pressed"); }, L`Demo`, true);"#,
    );
    assert!(
        lua.contains("BINDING_NAME_Demo_TOGGLE = __selu_wow.text"),
        "{lua}"
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[
        ("main.lua".into(), lua),
        ("press.lua".into(), "Demo_TOGGLE_Binding()".into()),
    ])
    .unwrap();
    assert_eq!(sim.prints(), ["pressed"]);
}
#[test]
fn invalid_declarations() {
    for (source, code) in [
        ("keybinding();", "SELU0660"),
        (r#"keybinding("bad-name", "Label", () => {});"#, "SELU0661"),
        (r#"keybinding("Demo_A", 1, () => {});"#, "SELU0663"),
        (
            r#"keybinding("Demo_A", "Label", (x: number) => {});"#,
            "SELU0664",
        ),
        (
            r#"keybinding("Demo_A", "Label", () => {}, "Demo", 1);"#,
            "SELU0665",
        ),
    ] {
        assert!(common::codes(source).iter().any(|c| c == code), "{source}");
    }
}
#[test]
fn xml_calls_global() {
    let xml = selu_lower::bindings::xml(&[selu_lower::bindings::Binding {
        span: selu_syntax::parse("t.selu", "1;").unwrap().stmts[0]
            .span
            .clone(),
        name: "Demo_A".into(),
        header: true,
        run_on_up: true,
    }]);
    assert_eq!(
        xml,
        "<Bindings>\n  <Binding name=\"Demo_A\" header=\"Demo_A\" runOnUp=\"true\">\n    Demo_A_Binding()\n  </Binding>\n</Bindings>\n"
    );
}

#[test]
fn project_layouts_names_and_simulator_loader() {
    use std::{fs, process::Command};
    let root = std::env::temp_dir().join(format!("selu-bindings-{}", std::process::id()));
    fs::create_dir_all(root.join("src")).unwrap();
    let manifests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests");
    let build = || {
        Command::new(env!("CARGO_BIN_EXE_selu"))
            .arg("build")
            .arg(&root)
            .arg("--manifest-dir")
            .arg(&manifests)
            .output()
            .unwrap()
    };
    for layout in ["per-target", "shared"] {
        fs::write(root.join("selu.toml"), format!("[addon]\nname = \"Demo\"\ntargets = [\"wow\", \"wowf\"]\n[build]\nlayout = \"{layout}\"\n")).unwrap();
        fs::write(
            root.join("src/main.selu"),
            r#"keybinding("Demo_A", "Action", () => { print("pressed"); });"#,
        )
        .unwrap();
        let o = build();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let folders = if layout == "shared" {
            vec![root.join("dist/Demo")]
        } else {
            vec![root.join("dist/Demo-wow"), root.join("dist/Demo-wowf")]
        };
        for folder in folders {
            let xml = fs::read_to_string(folder.join("Bindings.xml")).unwrap();
            assert!(xml.contains("Demo_A_Binding()"));
            let toc = fs::read_to_string(folder.join("Demo.toc")).unwrap();
            assert!(!toc.contains("Bindings.xml"));
            let sim = selu_sim::Sim::new().unwrap();
            let toc = selu_sim::loader::parse_toc("Demo", &folder, &toc);
            let files = selu_sim::loader::read_files(&toc).unwrap();
            sim.load_addon("Demo", &files).unwrap();
        }
        fs::write(
            root.join("src/main.selu"),
            r#"keybinding("Other_A", "Action", () => {});"#,
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&build().stderr).contains("SELU0662"));
        fs::write(
            root.join("src/main.selu"),
            r#"keybinding("Demo_A", "Action", () => {}); keybinding("Demo_A", "Again", () => {});"#,
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&build().stderr).contains("SELU0666"));
        fs::write(
            root.join("src/other.selu"),
            r#"export const registered = 1; keybinding("Demo_A", "Other", () => {});"#,
        )
        .unwrap();
        fs::write(root.join("src/main.selu"), "import { registered } from \"./other\"; print(registered); keybinding(\"Demo_A\", \"Action\", () => {});").unwrap();
        assert!(String::from_utf8_lossy(&build().stderr).contains("SELU0666"));
        fs::remove_file(root.join("src/other.selu")).unwrap();
    }
}

#[test]
fn generated_public_names_cannot_be_shadowed() {
    for declaration in [
        "function Demo_A_Binding() {}",
        "const BINDING_NAME_Demo_A = \"stolen\";",
        "let BINDING_HEADER_Demo_A = \"stolen\";",
    ] {
        for source in [
            format!("{declaration} keybinding(\"Demo_A\", \"Action\", () => {{}}, \"Demo\");"),
            format!("keybinding(\"Demo_A\", \"Action\", () => {{}}, \"Demo\"); {declaration}"),
        ] {
            assert!(
                common::codes(&source).iter().any(|c| c == "SELU0667"),
                "{source}"
            );
        }
    }
}
#[test]
fn declarations_must_be_module_statements() {
    for source in [
        r#"function setup() { keybinding("Demo_A", "Action", () => {}); }"#,
        r#"on PLAYER_LOGIN() { keybinding("Demo_A", "Action", () => {}); }"#,
        r#"if (false) { keybinding("Demo_A", "Action", () => {}); }"#,
        r#"{ keybinding("Demo_A", "Action", () => {}); }"#,
        r#"const ignored = keybinding("Demo_A", "Action", () => {});"#,
    ] {
        assert!(
            common::codes(source).iter().any(|c| c == "SELU0668"),
            "{source}"
        );
    }
}
#[test]
fn named_labels_headers_and_callbacks_work_from_another_chunk() {
    let lua = common::compile(
        r#"
        type Label = string;
        type Header = Label;
        type Action = () => void;
        const label: Label = "Action";
        const header: Header = "Demo";
        const action: Action = () => { print("pressed"); };
        keybinding("Demo_A", label, action, header);
    "#,
    );
    let sim = selu_sim::Sim::new().unwrap();
    sim.load_files(&[("main.lua".into(), lua), ("input.lua".into(), "assert(BINDING_NAME_Demo_A == \"Action\"); assert(BINDING_HEADER_Demo_A == \"Demo\"); Demo_A_Binding()".into())]).unwrap();
    assert_eq!(sim.prints(), ["pressed"]);
    assert!(common::codes(r#"type Action = (n: number) => void; const action: Action = (n: number) => {}; keybinding("Demo_A", "Action", action);"#).iter().any(|c| c == "SELU0664"));
}
