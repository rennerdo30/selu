//! Lua fields of one frame and typed list rows (extractions 36 and 37): a
//! field overlay owned by a frame global or keyed child, and `[[lists]]`
//! overlays that type a ScrollBox's rows and their element data. Checked
//! against the stored Forever manifests, which carry the Dialog Keys entries.
mod common;
use common::BoundedCommand;
use std::{path::PathBuf, process::Command};

fn check(label: &str, source: &str) -> std::process::Output {
    let root = std::env::temp_dir().join(format!("selu-lists-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("selu.toml"),
        "[addon]\nname = \"Lists\"\ntargets = [\"wowf\"]\n[build]\nstrict_api = true\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.selu"), source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_selu"))
        .arg("check")
        .arg(&root)
        .arg("--manifest-dir")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests"))
        .bounded_output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    out
}

fn text(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn frame_fields_and_typed_rows_check() {
    let out = check(
        "ok",
        r#"
function probe() {
  // A field Blizzard assigns on one frame, and on a template's frames.
  for (const [button, _active] of QuestFrameGreetingPanel.titleButtonPool.EnumerateActive()) {
    print(readable(button.GetID()));
  }
  const which: string? = StaticPopup1.which;
  print(which);
  // A list's rows carry their element data.
  for (const row of GossipFrame.GreetingPanel.ScrollBox.GetFrames()) {
    const data = row.GetElementData();
    const kind: number = data.buttonType;
    const info = data.info;
    if (info != null) { print(kind, info.questID, info.gossipOptionID); }
  }
  GossipFrame.GreetingPanel.ScrollBox.ForEachFrame((row) => { print(row.GetElementData().index); });
  // A child the frame gets from a shared template, named through the frame.
  for (const row of LootFrame.ScrollBox.GetFrames()) {
    const slot: number = row.GetElementData().slotIndex;
    print(slot);
  }
  print(GetCurrentKeyBoardFocus());
}
probe();
"#,
    );
    assert!(out.status.success(), "{}", text(&out));
}

#[test]
fn rows_of_other_lists_and_plain_frames_stay_closed() {
    // The same template child on another frame has no typed rows, and a
    // plain frame has no element data.
    let out = check(
        "closed",
        r#"
function probe(frame: Frame) {
  print(frame.GetElementData());
}
probe(UIParent);
"#,
    );
    assert!(!out.status.success());
    assert!(text(&out).contains("SELU0582"), "{}", text(&out));
    // A frame field is read-only.
    let out = check(
        "readonly",
        r#"
function probe() { StaticPopup1.which = "X"; }
probe();
"#,
    );
    assert!(!out.status.success(), "{}", text(&out));
}
