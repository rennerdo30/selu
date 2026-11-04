use selu_manifest::Manifest;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("selu-f2-{tag}-{}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn manifest(flavour: &str, prefix: &str, widget: &str) -> Manifest {
    serde_json::from_value(serde_json::json!({"flavour":flavour,"build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1,"luaDialects":["lua51"],"functions":[{"name":"print","params":["..."],"evidence":"source"}],"events":[{"name":"PLAYER_LOGIN","evidence":"source"}],"widgets":[{"name":"Frame"},{"name":"Button"}],"frameFamilies":[{"prefix":prefix,"suffix":"","minIndex":1,"maxIndex":3,"widget":widget}]})).unwrap()
}
fn build(root: &Path, name: &str, manifests: &[Manifest], shared: bool, source: &str) -> PathBuf {
    let project = root.join(name);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir(project.join("manifests")).unwrap();
    let targets: Vec<_> = manifests.iter().map(|m| m.flavour.clone()).collect();
    let mut config = format!(
        "[addon]\nname={name:?}\ntargets={}\n",
        serde_json::to_string(&targets).unwrap()
    );
    if shared {
        config.push_str("[build]\nlayout=\"shared\"\n");
    }
    std::fs::write(project.join("selu.toml"), config).unwrap();
    std::fs::write(project.join("src/main.selu"), source).unwrap();
    for m in manifests {
        std::fs::write(
            project
                .join("manifests")
                .join(format!("{}.json", m.flavour)),
            serde_json::to_vec(m).unwrap(),
        )
        .unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_selu"))
        .current_dir(root)
        .env_remove("SELU_MANIFEST_DIR")
        .arg("build")
        .arg(&project)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    eprintln!("build PID={}", child.id());
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("build timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    project.join("dist").join(name)
}
#[test]
fn shared_snapshots_select_and_validate_the_session_target() {
    let root = Scratch::new("shared-snapshots");
    let addon = build(
        &root.0,
        "Shared",
        &[
            manifest("retail", "OnlyRetail", "Frame"),
            manifest("classic", "OnlyClassic", "Frame"),
        ],
        true,
        "print(\"shared\");",
    );
    let path = selu_manifest::resolution::compiled_manifest_path(&addon);
    let snapshots: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(snapshots["manifests"].as_array().unwrap().len(), 2);
    assert!(selu_manifest::resolution::load_checked_manifest(&path, Some("retail@2")).is_err());
    for (target, yes, no) in [
        ("retail", "OnlyRetail", "OnlyClassic"),
        ("classic", "OnlyClassic", "OnlyRetail"),
    ] {
        let mut s = selu_sim::Session::new(selu_sim::SessionConfig {
            target: Some(target.into()),
            ..Default::default()
        })
        .unwrap();
        s.load_addon_dir(&addon).unwrap();
        for reload in [false, true] {
            if reload {
                s.reload().unwrap();
            }
            s.eval(&format!(
                "assert(_G['{yes}2']==nil); assert(_G['{no}2']~=nil)"
            ))
            .unwrap();
        }
    }
    let mut wrong = selu_sim::Session::new(selu_sim::SessionConfig {
        target: Some("forever".into()),
        ..Default::default()
    })
    .unwrap();
    assert!(wrong.load_addon_dir(&addon).is_err());
    assert!(wrong.prints().is_empty());
    let mut ambiguous = selu_sim::Session::new(Default::default()).unwrap();
    assert!(ambiguous.load_addon_dir(&addon).is_err());
}
#[test]
fn multiple_addon_snapshots_merge_reject_conflicts_and_reload_together() {
    let root = Scratch::new("multiple-snapshots");
    let a = build(
        &root.0,
        "A",
        &[manifest("retail", "OnlyA", "Frame")],
        false,
        "on PLAYER_LOGIN() { print(frameFamily(\"OnlyA\",2)==null); }",
    );
    let b = build(
        &root.0,
        "B",
        &[manifest("retail", "OnlyB", "Frame")],
        false,
        "on PLAYER_LOGIN() { print(frameFamily(\"OnlyB\",2)==null); }",
    );
    let conflict = build(
        &root.0,
        "Conflict",
        &[manifest("retail", "OnlyA", "Button")],
        false,
        "print(\"must not run\");",
    );
    let mut s = selu_sim::Session::new(selu_sim::SessionConfig {
        target: Some("retail".into()),
        ..Default::default()
    })
    .unwrap();
    s.load_addon_dir(&a).unwrap();
    s.load_addon_dir(&b).unwrap();
    s.login().unwrap();
    assert_eq!(s.prints(), vec!["true", "true"]);
    let err = s.load_addon_dir(&conflict).unwrap_err().to_string();
    assert!(err.contains("incompatible"), "{err}");
    s.reload().unwrap();
    assert_eq!(s.prints(), vec!["true", "true"]);
    let path = selu_manifest::resolution::compiled_manifest_path(&b);
    let original = std::fs::read(&path).unwrap();
    let mut wrong: serde_json::Value = serde_json::from_slice(&original).unwrap();
    wrong["flavour"] = "classic".into();
    std::fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(s.reload().is_err());
    s.eval("assert(_G['OnlyA2']==nil and _G['OnlyB2']==nil)")
        .unwrap();
    std::fs::write(&path, original).unwrap();
    s.reload().unwrap();
    assert_eq!(s.prints(), vec!["true", "true"]);
}
