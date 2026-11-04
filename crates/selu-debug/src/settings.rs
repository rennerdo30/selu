//! Project Settings: a form over `selu.toml` (addon name, targets with the
//! flavours and builds the manifests know, build options, formatting).
//! Saving edits the file in place with `toml_edit`, so comments, key order
//! and unknown keys survive; the editor's background check then validates
//! the result with the real compiler.

use crate::theme::{caption, icon, icon_text, palette};
use eframe::egui::{self, Color32, RichText};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

/// One target row: flavour plus an optional version (`retail`, `retail@12.0`).
#[derive(Debug, Clone, PartialEq)]
pub struct TargetRow {
    pub flavour: String,
    /// Empty means "newest build of the flavour".
    pub version: String,
}

impl TargetRow {
    pub fn parse(s: &str) -> TargetRow {
        match s.split_once('@') {
            Some((f, v)) => TargetRow {
                flavour: f.trim().into(),
                version: v.trim().into(),
            },
            None => TargetRow {
                flavour: s.trim().into(),
                version: String::new(),
            },
        }
    }
    pub fn render(&self) -> String {
        if self.version.is_empty() {
            self.flavour.clone()
        } else {
            format!("{}@{}", self.flavour, self.version)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub name: String,
    pub targets: Vec<TargetRow>,
    pub shared_layout: bool,
    pub strict_api: bool,
    pub indent: usize,
    pub format_on_save: bool,
}

impl Settings {
    pub fn read(doc: &DocumentMut) -> Settings {
        let addon = doc.get("addon");
        let build = doc.get("build");
        let format = doc.get("format");
        Settings {
            name: addon
                .and_then(|a| a.get("name"))
                .and_then(Item::as_str)
                .unwrap_or("")
                .to_string(),
            targets: addon
                .and_then(|a| a.get("targets"))
                .and_then(Item::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(TargetRow::parse)
                        .collect()
                })
                .unwrap_or_default(),
            shared_layout: build.and_then(|b| b.get("layout")).and_then(Item::as_str)
                == Some("shared"),
            strict_api: build
                .and_then(|b| b.get("strict_api"))
                .and_then(Item::as_bool)
                .unwrap_or(false),
            indent: format
                .and_then(|f| f.get("indent"))
                .and_then(Item::as_integer)
                .map_or(2, |n| n.clamp(1, 8) as usize),
            format_on_save: format
                .and_then(|f| f.get("on_save"))
                .and_then(Item::as_bool)
                .unwrap_or(false),
        }
    }

    /// Write the values into `doc`, touching only keys that differ from
    /// their defaults or already exist. Unchanged values keep their exact
    /// text; changed ones keep the comments and spacing around them.
    pub fn write(&self, doc: &mut DocumentMut) {
        fn set(table: &mut Table, key: &str, new: Value) {
            if let Some(old) = table.get(key).and_then(Item::as_value) {
                if old.to_string().trim() == new.to_string().trim() {
                    return;
                }
                let decor = old.decor().clone();
                let mut new = new;
                *new.decor_mut() = decor;
                table[key] = Item::Value(new);
            } else {
                table[key] = Item::Value(new);
            }
        }
        fn table<'a>(doc: &'a mut DocumentMut, name: &str) -> &'a mut Table {
            if !doc.contains_table(name) {
                doc.insert(name, Item::Table(Table::new()));
            }
            doc[name].as_table_mut().expect("just inserted a table")
        }
        let addon = table(doc, "addon");
        set(addon, "name", Value::from(self.name.trim()));
        let mut arr = Array::new();
        for t in &self.targets {
            if !t.flavour.trim().is_empty() {
                arr.push(t.render());
            }
        }
        set(addon, "targets", Value::Array(arr));

        let layout = if self.shared_layout {
            Some("shared")
        } else {
            None
        };
        let has_build = doc.contains_table("build");
        if has_build || layout.is_some() || self.strict_api {
            let build = table(doc, "build");
            match layout {
                Some(l) => set(build, "layout", Value::from(l)),
                None => {
                    build.remove("layout");
                }
            }
            if self.strict_api {
                set(build, "strict_api", Value::from(true));
            } else {
                build.remove("strict_api");
            }
            if build.is_empty() {
                doc.remove("build");
            }
        }
        let has_format = doc.contains_table("format");
        if has_format || self.indent != 2 || self.format_on_save {
            let format = table(doc, "format");
            if self.indent != 2 {
                set(format, "indent", Value::from(self.indent as i64));
            } else {
                format.remove("indent");
            }
            if self.format_on_save {
                set(format, "on_save", Value::from(true));
            } else {
                format.remove("on_save");
            }
            if format.is_empty() {
                doc.remove("format");
            }
        }
    }

    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let name = self.name.trim();
        if name.is_empty() {
            out.push("The addon needs a name.".into());
        } else if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            out.push("Use letters, digits, `_` or `-` in the addon name (it becomes the folder and .toc name).".into());
        }
        if self.targets.iter().all(|t| t.flavour.trim().is_empty()) {
            out.push("Add at least one target.".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for t in &self.targets {
            if !t.flavour.is_empty() && !seen.insert(t.flavour.clone()) {
                out.push(format!(
                    "`{}` is listed twice; one build per flavour.",
                    t.flavour
                ));
            }
        }
        out
    }
}

/// Flavours and their builds, from the manifest file names
/// (`manifests/retail/12.1.5.70391.selum`).
pub fn known_builds(manifests: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(manifests) else {
        return out;
    };
    for e in entries.flatten() {
        let dir = e.path();
        let Some(flavour) = dir.file_name().and_then(|n| n.to_str()).map(String::from) else {
            continue;
        };
        if !dir.is_dir() || flavour == "fixtures" {
            continue;
        }
        let mut builds: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|f| {
                let name = f.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".selum")
                    .or_else(|| name.strip_suffix(".json"))
                    .map(String::from)
            })
            .collect();
        builds.sort();
        out.insert(flavour, builds);
    }
    out
}

/// Version choices for a build such as `12.1.5.70391`: the newest build
/// (empty), the major.minor, the patch, and the exact build.
fn version_choices(builds: &[String]) -> Vec<(String, String)> {
    let mut out = vec![(String::new(), "newest build".to_string())];
    for b in builds.iter().rev() {
        let parts: Vec<&str> = b.split('.').collect();
        for n in [2, 3, 4] {
            if parts.len() >= n {
                let v = parts[..n].join(".");
                if !out.iter().any(|(x, _)| *x == v) {
                    let label = match n {
                        2 => format!("{v} (any {v}.x)"),
                        3 => format!("{v} (any build)"),
                        _ => format!("{v} (exact)"),
                    };
                    out.push((v, label));
                }
            }
        }
    }
    out
}

pub struct SettingsWindow {
    path: PathBuf,
    doc: DocumentMut,
    pub settings: Settings,
    original: Settings,
    builds: BTreeMap<String, Vec<String>>,
    error: Option<String>,
}

pub enum Outcome {
    Open,
    Saved,
    Closed,
}

impl SettingsWindow {
    pub fn open(project: &Path) -> Result<SettingsWindow, String> {
        let path = project.join("selu.toml");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let doc: DocumentMut = text
            .parse()
            .map_err(|e| format!("selu.toml is not valid TOML: {e}"))?;
        let settings = Settings::read(&doc);
        let builds = crate::editor::complete::find_manifests(project)
            .map(|m| known_builds(&m))
            .unwrap_or_default();
        Ok(SettingsWindow {
            path,
            doc,
            original: settings.clone(),
            settings,
            builds,
            error: None,
        })
    }

    fn save(&mut self) -> bool {
        self.settings.write(&mut self.doc);
        match std::fs::write(&self.path, self.doc.to_string()) {
            Ok(()) => {
                self.original = self.settings.clone();
                true
            }
            Err(e) => {
                self.error = Some(format!("cannot save: {e}"));
                false
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Outcome {
        let mut outcome = Outcome::Open;
        let mut open = true;
        egui::Window::new(RichText::new("Project Settings").size(15.0).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let s = &mut self.settings;
                ui.label(
                    RichText::new(self.path.display().to_string())
                        .small()
                        .color(palette::FAINT),
                );
                ui.add_space(10.0);

                caption(ui, "Addon");
                ui.add_space(2.0);
                egui::Grid::new("addon").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(260.0))
                        .on_hover_text("The addon's folder and .toc name in the game");
                    ui.end_row();
                });
                ui.add_space(12.0);

                caption(ui, "Targets");
                ui.label(
                    RichText::new("The game clients this addon is built for; each gets its own checks and output.")
                        .small()
                        .color(palette::MUTED),
                );
                ui.add_space(4.0);
                let flavours: Vec<String> = self.builds.keys().cloned().collect();
                let mut remove = None;
                for (i, t) in s.targets.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("flavour", i))
                            .width(180.0)
                            .selected_text(if t.flavour.is_empty() { "choose..." } else { &t.flavour })
                            .show_ui(ui, |ui| {
                                for f in &flavours {
                                    if ui.selectable_label(&t.flavour == f, f).clicked() {
                                        t.flavour = f.clone();
                                        t.version.clear();
                                    }
                                }
                            });
                        let builds = self.builds.get(&t.flavour).cloned().unwrap_or_default();
                        let choices = version_choices(&builds);
                        let current = choices
                            .iter()
                            .find(|(v, _)| *v == t.version)
                            .map_or_else(|| t.version.clone(), |(_, l)| l.clone());
                        egui::ComboBox::from_id_salt(("version", i))
                            .width(220.0)
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (v, label) in &choices {
                                    if ui.selectable_label(&t.version == v, label).clicked() {
                                        t.version = v.clone();
                                    }
                                }
                            });
                        if ui
                            .add(egui::Button::new(icon_text(icon::CLOSE, 10.0)).frame(false))
                            .on_hover_text("Remove this target")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                    if !t.flavour.is_empty() && !self.builds.contains_key(&t.flavour) {
                        ui.label(
                            RichText::new(format!("No manifest for `{}`.", t.flavour))
                                .small()
                                .color(palette::WARNING),
                        );
                    }
                }
                if let Some(i) = remove {
                    s.targets.remove(i);
                }
                if ui.button("Add target").clicked() {
                    let next = flavours
                        .iter()
                        .find(|f| !s.targets.iter().any(|t| &t.flavour == *f))
                        .cloned()
                        .unwrap_or_default();
                    s.targets.push(TargetRow {
                        flavour: next,
                        version: String::new(),
                    });
                }
                ui.add_space(12.0);

                caption(ui, "Build");
                ui.add_space(2.0);
                ui.add_enabled_ui(s.targets.len() > 1, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Output");
                        ui.radio_value(&mut s.shared_layout, false, "One folder per target");
                        ui.radio_value(&mut s.shared_layout, true, "One shared folder");
                    })
                    .response
                    .on_disabled_hover_text("Only matters with several targets");
                });
                ui.checkbox(&mut s.strict_api, "Strict game API checks")
                    .on_hover_text("Unknown or unavailable API calls are errors instead of warnings");
                ui.add_space(12.0);

                caption(ui, "Formatting");
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label("Indent");
                    ui.radio_value(&mut s.indent, 2, "2 spaces");
                    ui.radio_value(&mut s.indent, 4, "4 spaces");
                });
                ui.checkbox(&mut s.format_on_save, "Format files when saving");
                ui.add_space(12.0);

                let problems = s.problems();
                for p in &problems {
                    ui.label(RichText::new(p).color(palette::ERROR));
                }
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).color(palette::ERROR));
                }
                crate::theme::rule(ui);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let dirty = self.settings != self.original;
                    let save = egui::Button::new(RichText::new("Save").color(Color32::WHITE))
                        .fill(palette::ACCENT);
                    if ui.add_enabled(dirty && problems.is_empty(), save).clicked() && self.save() {
                        outcome = Outcome::Saved;
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Outcome::Closed;
                    }
                    if dirty {
                        ui.label(RichText::new("unsaved changes").small().color(palette::MUTED));
                    }
                });
            });
        if !open {
            outcome = Outcome::Closed;
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_comments_and_unknown_keys() {
        let text = "# My addon\n[addon]\nname = \"Demo\" # shown in game\ntargets = [\"retail@12.0\"]\n\n[extra]\nkeep = 1\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let mut s = Settings::read(&doc);
        assert_eq!(s.name, "Demo");
        assert_eq!(
            s.targets,
            [TargetRow {
                flavour: "retail".into(),
                version: "12.0".into()
            }]
        );
        s.targets.push(TargetRow::parse("classic-era"));
        s.shared_layout = true;
        s.indent = 4;
        s.write(&mut doc);
        let out = doc.to_string();
        assert!(out.starts_with("# My addon\n"), "{out}");
        assert!(out.contains("# shown in game"), "{out}");
        assert!(
            out.contains("targets = [\"retail@12.0\", \"classic-era\"]"),
            "{out}"
        );
        assert!(out.contains("[build]\nlayout = \"shared\""), "{out}");
        assert!(out.contains("[format]\nindent = 4"), "{out}");
        assert!(out.contains("[extra]\nkeep = 1"), "{out}");
        // Back to defaults removes the tables again.
        let mut s = Settings::read(&out.parse().unwrap());
        s.shared_layout = false;
        s.indent = 2;
        let mut doc: DocumentMut = out.parse().unwrap();
        s.write(&mut doc);
        let out = doc.to_string();
        assert!(
            !out.contains("[build]") && !out.contains("[format]"),
            "{out}"
        );
    }

    #[test]
    fn version_choices_cover_major_patch_and_exact() {
        let c = version_choices(&["12.1.5.70391".to_string()]);
        let values: Vec<&str> = c.iter().map(|(v, _)| v.as_str()).collect();
        assert_eq!(values, ["", "12.1", "12.1.5", "12.1.5.70391"]);
        let builds = known_builds(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests"));
        assert!(builds.contains_key("retail") && !builds.contains_key("fixtures"));
    }

    #[test]
    fn problems_are_reported() {
        let mut s = Settings::read(&"[addon]\nname = \"\"\n".parse().unwrap());
        assert_eq!(s.problems().len(), 2);
        s.name = "Bad Name".into();
        s.targets = vec![TargetRow::parse("retail"), TargetRow::parse("retail@12.0")];
        let p = s.problems();
        assert!(
            p.iter().any(|x| x.contains("letters")) && p.iter().any(|x| x.contains("twice")),
            "{p:?}"
        );
    }
}
