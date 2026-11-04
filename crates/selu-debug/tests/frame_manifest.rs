//! The real headless Studio binary uses the same Session path as the window.
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn external_manifest_override_reaches_studio_and_survives_reload() {
    let root = std::env::temp_dir().join(format!("selu-f2-studio-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let addon = root.join("External");
    std::fs::create_dir(&addon).unwrap();
    let dir = root.join("override");
    std::fs::create_dir(&dir).unwrap();
    let file = dir.join("target.json");
    std::fs::write(&file,r#"{"flavour":"retail","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1,"frameFamilies":[{"prefix":"MerchantItem","suffix":"","minIndex":1,"maxIndex":12,"widget":"Frame"}]}"#).unwrap();
    std::fs::write(addon.join("External.toc"), "main.lua\n").unwrap();
    std::fs::write(addon.join("main.lua"), "print(_G['MerchantItem2']==nil)").unwrap();
    for explicit in [true, false] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_selu-studio"));
        cmd.current_dir(&root)
            .arg("--headless")
            .args(["--target", "retail"])
            .arg(&addon);
        if explicit {
            cmd.arg("--manifest")
                .arg(&file)
                .env_remove("SELU_MANIFEST_DIR");
        } else {
            cmd.env("SELU_MANIFEST_DIR", &dir);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        eprintln!("headless Studio PID={}", child.id());
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"{\"command\":\"reload\"}\n")
            .unwrap();
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Studio timeout");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let updates: Vec<serde_json::Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert!(
            !updates.iter().any(|u| u["update"] == "error"),
            "{updates:?}"
        );
        let prints: Vec<_> = updates
            .iter()
            .filter(|u| u["update"] == "prints")
            .flat_map(|u| u["lines"].as_array().unwrap())
            .collect();
        assert_eq!(prints.len(), 2, "{updates:?}");
        assert!(
            prints.iter().all(|s| s.as_str() == Some("true")),
            "{updates:?}"
        );
    }
}
