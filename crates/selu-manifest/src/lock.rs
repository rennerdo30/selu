//! Project lock files retain exact targets until an explicit update.
use crate::{Manifest, TargetSpec};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The project's declared target list, shared by tooling and offline proof harnesses.
pub fn project_targets(project: &Path) -> Result<Vec<String>> {
    let config: toml::Value = toml::from_str(&std::fs::read_to_string(project.join("selu.toml"))?)?;
    config
        .get("addon")
        .and_then(|a| a.get("targets"))
        .and_then(toml::Value::as_array)
        .ok_or("missing addon.targets")?
        .iter()
        .map(|target| {
            target
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "addon.targets must contain strings".into())
        })
        .collect()
}

fn target_key(spec: &TargetSpec) -> String {
    let version = spec
        .parts
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".");
    format!(
        "{}{}",
        crate::product(&spec.flavour),
        if version.is_empty() {
            String::new()
        } else {
            format!("@{version}")
        }
    )
}

fn widget_methods(
    manifest: &Manifest,
    name: &str,
) -> Result<BTreeMap<String, Vec<serde_json::Value>>> {
    let mut methods = BTreeMap::<String, Vec<serde_json::Value>>::new();
    let mut pending = vec![name.to_owned()];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(widget) = manifest.widgets.iter().find(|w| w.name == name) {
            pending.extend(widget.inherits.iter().cloned());
            for method in &widget.methods {
                methods
                    .entry(method.name.clone())
                    .or_default()
                    .push(serde_json::to_value(method)?);
            }
        }
    }
    Ok(methods)
}

fn reference_names(value: &serde_json::Value, names: &mut std::collections::BTreeSet<String>) {
    for key in ["name", "prefix"] {
        if let Some(name) = value.get(key).and_then(serde_json::Value::as_str)
            && !name.is_empty()
        {
            names.insert(name.into());
        }
    }
    for key in ["fields", "methods", "predicates"] {
        for member in value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(name) = member.as_str() {
                names.insert(name.into());
            } else {
                reference_names(member, names);
            }
        }
    }
    if let Some(values) = value.get("values").and_then(serde_json::Value::as_object) {
        names.extend(values.keys().cloned());
    }
}

/// Complete API and candidate-source diff shared by game-build and extraction updates.
pub fn api_diff(project: &Path, old: &Manifest, newest: &Manifest) -> Result<String> {
    let mut report = String::new();
    let old_value = serde_json::to_value(old)?;
    let new_value = serde_json::to_value(newest)?;
    for section in [
        "functions",
        "events",
        "widgets",
        "globals",
        "tables",
        "predicates",
        "enums",
        "templates",
        "dataGlobals",
        "frameFamilies",
    ] {
        let entries = |value: &serde_json::Value| -> BTreeMap<String, serde_json::Value> {
            value[section]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| {
                    let name = if section == "frameFamilies" {
                        format!(
                            "{}...{}",
                            v.get("prefix")?.as_str()?,
                            v.get("suffix")?.as_str()?
                        )
                    } else if section == "enums" {
                        format!("Enum.{}", v.get("name")?.as_str()?)
                    } else {
                        v.get("name")?.as_str()?.to_owned()
                    };
                    Some((name, v.clone()))
                })
                .collect()
        };
        let before = entries(&old_value);
        let after = entries(&new_value);
        let added = after.keys().filter(|k| !before.contains_key(*k)).count();
        let removed: Vec<_> = before.keys().filter(|k| !after.contains_key(*k)).collect();
        let changed: Vec<_> = before
            .keys()
            .filter(|k| after.get(*k).is_some_and(|v| v != &before[*k]))
            .collect();
        report.push_str(&format!(
            "  {section}: +{added}, -{}, changed {}\n",
            removed.len(),
            changed.len()
        ));
        for name in removed.iter().chain(&changed) {
            report.push_str(&format!("    {name}\n"));
        }
        let mut affected: std::collections::BTreeSet<String> = removed
            .iter()
            .chain(&changed)
            .map(|name| (*name).clone())
            .collect();
        for name in removed.iter().chain(&changed) {
            reference_names(&before[*name], &mut affected);
        }
        if section == "widgets" {
            for widget in removed.iter().chain(&changed) {
                let before = widget_methods(old, widget)?;
                let after = widget_methods(newest, widget)?;
                for (name, declarations) in before {
                    if after.get(&name) != Some(&declarations) {
                        affected.insert(name);
                    }
                }
            }
        }
        let mut stack = vec![project.join("src")];
        while let Some(dir) = stack.pop() {
            if !dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !path.extension().is_some_and(|e| e == "selu") {
                    continue;
                }
                for (line, text) in std::fs::read_to_string(&path)?.lines().enumerate() {
                    if affected.iter().any(|name| text.contains(name.as_str())) {
                        report.push_str(&format!(
                            "    possible use {}:{}: {}\n",
                            path.display(),
                            line + 1,
                            text.trim()
                        ));
                    }
                }
            }
        }
    }
    Ok(report)
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    pub version: u32,
    pub targets: BTreeMap<String, String>,
}
impl Lock {
    /// Prepare an explicit update; callers display the report before confirming.
    pub fn update(
        &mut self,
        project: &Path,
        manifests: &[Manifest],
        products: &[String],
        yes: bool,
    ) -> Result<String> {
        let targets = project_targets(project)?;
        let valid = targets
            .iter()
            .map(|target| {
                TargetSpec::parse(target)
                    .map(|spec| crate::product(&spec.flavour).to_owned())
                    .map_err(|e| e.to_string())
            })
            .collect::<std::result::Result<std::collections::BTreeSet<_>, _>>()?;
        for selector in products {
            if !valid.contains(crate::product(selector)) {
                return Err(format!("unknown update selector `{selector}`; valid project products: {}; project targets: {}",valid.iter().cloned().collect::<Vec<_>>().join(", "),targets.join(", ")).into());
            }
        }
        let mut next = self.targets.clone();
        let mut report = String::new();
        let mut major = false;
        for target in &targets {
            let spec = TargetSpec::parse(target).map_err(|e| format!("{e:?}"))?;
            let product = crate::product(&spec.flavour);
            if !products.is_empty() && !products.iter().any(|p| crate::product(p) == product) {
                continue;
            }
            if spec.parts.len() == 4 {
                report.push_str(&format!("{target}: exact project pin retained\n"));
                continue;
            }
            let key = target_key(&spec);
            let newest = crate::resolve(&spec, manifests).map_err(|e| format!("{e:?}"))?;
            let old = self
                .targets
                .get(&key)
                .map(|target| {
                    let spec = TargetSpec::parse(target).map_err(|e| format!("{e:?}"))?;
                    crate::resolve(&spec, manifests).map_err(|e| format!("{e:?}"))
                })
                .transpose()?;
            report.push_str(&format!(
                "{product}: {} -> {}\n",
                old.map(|m| m.build.to_string())
                    .unwrap_or_else(|| "unlocked".into()),
                newest.build
            ));
            if old.is_some_and(|old| old.build.major != newest.build.major) {
                major = true;
                report.push_str("WARNING: MAJOR VERSION CHANGE - confirmation required\n");
            }
            if let Some(old) = old {
                report.push_str(&api_diff(project, old, newest)?);
            }
            next.insert(key, format!("{product}@{}", newest.build));
        }
        if major && !yes {
            return Err(
                format!("{report}confirm with --yes (or Studio's confirmation dialog)").into(),
            );
        }
        self.targets = next;
        self.write(project)?;
        Ok(report)
    }
    pub fn read(project: &Path) -> Result<Self> {
        match std::fs::read_to_string(project.join("selu.lock")) {
            Ok(text) => {
                let lock: Self = toml::from_str(&text)?;
                if lock.version != 1 {
                    return Err("unsupported selu.lock version".into());
                }
                Ok(lock)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                version: 1,
                ..Self::default()
            }),
            Err(e) => Err(e.into()),
        }
    }
    pub fn write(&self, project: &Path) -> Result<()> {
        std::fs::write(project.join("selu.lock"), toml::to_string_pretty(self)?)?;
        Ok(())
    }
    /// Discover locked snapshots directly, avoiding historical builds the project does not use.
    pub fn discovery_targets(&self, targets: &[String]) -> Result<Vec<String>> {
        targets
            .iter()
            .map(|target| {
                let spec = TargetSpec::parse(target).map_err(|e| e.to_string())?;
                Ok(self
                    .targets
                    .get(&target_key(&spec))
                    .cloned()
                    .unwrap_or_else(|| target.clone()))
            })
            .collect()
    }
    pub fn resolve(&mut self, targets: &[String], manifests: &[Manifest]) -> Result<Vec<Manifest>> {
        targets.iter().map(|target| {
            let requested = TargetSpec::parse(target).map_err(|e| format!("{e:?}"))?;
            let key = target_key(&requested);
            let resolved = if let Some(locked) = self.targets.get(&key) {
                let spec = TargetSpec::parse(locked).map_err(|e| format!("{e:?}"))?;
                if spec.parts.len() != 4 || crate::product(&spec.flavour) != crate::product(&requested.flavour) || (requested.parts.len() == 4 && requested.parts != spec.parts) {
                    return Err(format!("invalid locked target for {key}").into());
                }
                crate::resolve(&spec, manifests).map_err(|_| format!("locked build {locked} missing from store; fetch the wow-ui-source clone, then run selu-manifest-gen update --source <clone>"))?
            } else {
                crate::resolve(&requested, manifests).map_err(|e| format!("{e:?}"))?
            };
            self.targets.insert(key, format!("{}@{}", crate::product(&resolved.flavour), resolved.build));
            Ok(resolved.clone())
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_reports_removed_global_and_every_other_consumed_section() {
        let project =
            std::env::temp_dir().join(format!("selu-all-api-diff-{}", std::process::id()));
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join("selu.toml"),
            "[addon]\nname=\"Diff\"\ntargets=[\"wow\"]\n",
        )
        .unwrap();
        let sections = [
            (
                "globals",
                "RemovedGlobal",
                serde_json::json!({"name":"RemovedGlobal","type":"number","evidence":"source"}),
                "const value = RemovedGlobal;",
            ),
            (
                "tables",
                "RemovedType",
                serde_json::json!({"name":"RemovedType","type":"Structure","fields":[{"name":"lost","type":"number"}]}),
                "let value: RemovedType;",
            ),
            (
                "predicates",
                "RemovedPredicate",
                serde_json::json!({"name":"RemovedPredicate","type":"number"}),
                "RemovedPredicate(value);",
            ),
            (
                "enums",
                "RemovedEnum",
                serde_json::json!({"name":"RemovedEnum","values":{"Lost":1}}),
                "const value = Enum.RemovedEnum.Lost;",
            ),
            (
                "templates",
                "RemovedTemplate",
                serde_json::json!({"name":"RemovedTemplate","source":0,"evidence":"source","methods":["DoOld"]}),
                "const frame = CreateFrame(\"Frame\", undefined, undefined, \"RemovedTemplate\");",
            ),
            (
                "dataGlobals",
                "RemovedFrame",
                serde_json::json!({"name":"RemovedFrame","source":0,"evidence":"source","type":"Frame"}),
                "RemovedFrame.Show();",
            ),
            (
                "frameFamilies",
                "Gone",
                serde_json::json!({"prefix":"Gone","suffix":"Button","minIndex":1,"maxIndex":2,"widget":"Button"}),
                "frameFamily(\"Gone\",1,\"Button\");",
            ),
        ];
        for (section, name, entry, source) in sections {
            let mut json = serde_json::to_value(manifest(1)).unwrap();
            json["sourceFiles"] = serde_json::json!(["source.lua"]);
            json[section] = serde_json::json!([entry]);
            let old: Manifest = serde_json::from_value(json).unwrap();
            let new = manifest(2);
            std::fs::write(project.join("src/main.selu"), source).unwrap();
            let mut lock = Lock::read(&project).unwrap();
            lock.targets.clear();
            lock.resolve(&["wow".into()], std::slice::from_ref(&old))
                .unwrap();
            lock.write(&project).unwrap();
            let report = lock.update(&project, &[old, new], &[], false).unwrap();
            assert!(
                report.contains(&format!("{section}: +0, -1")),
                "{section}: {report}"
            );
            assert!(
                report.contains(name) && report.contains("main.selu:1:") && report.contains(source),
                "{report}"
            );
            assert_eq!(Lock::read(&project).unwrap().targets["wow"], "wow@12.1.5.2");
        }
        std::fs::remove_dir_all(project).unwrap();
    }
    #[test]
    fn unknown_update_selector_errors_without_moving_any_lock() {
        let project =
            std::env::temp_dir().join(format!("selu-unknown-selector-{}", std::process::id()));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("selu.toml"),
            "[addon]\nname=\"Typo\"\ntargets=[\"wow\"]\n",
        )
        .unwrap();
        let mut lock = Lock::read(&project).unwrap();
        lock.resolve(&["wow".into()], &[manifest(1)]).unwrap();
        lock.write(&project).unwrap();
        let before = std::fs::read(project.join("selu.lock")).unwrap();
        for selectors in [
            vec!["wow_typo".into()],
            vec!["wowf".into()],
            vec!["wow".into(), "wow_typo".into()],
        ] {
            let error = lock
                .update(&project, &[manifest(1), manifest(2)], &selectors, false)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(selectors.last().unwrap())
                    && error.contains("wow")
                    && error.contains("valid"),
                "{error}"
            );
            assert_eq!(lock.targets["wow"], "wow@12.1.5.1");
            assert_eq!(std::fs::read(project.join("selu.lock")).unwrap(), before);
        }
        std::fs::remove_dir_all(project).unwrap();
    }
    fn manifest(build: u32) -> Manifest {
        serde_json::from_value(serde_json::json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":build},"interface":120105})).unwrap()
    }
    #[test]
    fn legacy_partial_selector_updates_within_its_version_line() {
        let project = std::env::temp_dir().join(format!(
            "selu-legacy-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("selu.toml"),
            "[addon]\nname=\"Legacy\"\ntargets=[\"retail@12.1\"]\n",
        )
        .unwrap();
        let targets = ["retail@12.1".into()];
        let old = manifest(1);
        let mut next = manifest(2);
        next.build.patch += 1;
        let mut other_line = manifest(3);
        other_line.build.major += 1;
        let mut lock = Lock::read(&project).unwrap();
        lock.resolve(&targets, std::slice::from_ref(&old)).unwrap();
        lock.write(&project).unwrap();
        let report = lock
            .update(&project, &[old, next, other_line], &["wow".into()], false)
            .unwrap();
        assert!(report.contains("12.1.5.1 -> 12.1.6.2"), "{report}");
        assert_eq!(
            Lock::read(&project).unwrap().targets["wow@12.1"],
            "wow@12.1.6.2"
        );
        assert!(
            std::fs::canonicalize(&project)
                .unwrap()
                .starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap())
        );
        std::fs::remove_dir_all(project).unwrap();
    }
    #[test]
    fn major_updates_require_confirmation_and_show_changed_member_uses() {
        let project = std::env::temp_dir().join(format!(
            "selu-lock-major-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join("selu.toml"),
            "[addon]\nname=\"Example\"\ntargets=[\"wow\"]\n",
        )
        .unwrap();
        std::fs::write(
            project.join("src/main.selu"),
            "const f = CreateFrame(\"Frame\");\nf.Run(1);\n",
        )
        .unwrap();
        let mut old = manifest(1);
        old.widgets.push(serde_json::from_value(serde_json::json!({"name":"Frame","methods":[{"name":"Run","arguments":[{"name":"value","type":"number"}]}]})).unwrap());
        let mut new = old.clone();
        new.build.major += 1;
        new.widgets[0].methods[0].arguments[0].ty = Some("string".into());
        let mut lock = Lock::read(&project).unwrap();
        lock.resolve(&["wow".into()], std::slice::from_ref(&old))
            .unwrap();
        lock.write(&project).unwrap();
        let before = std::fs::read(project.join("selu.lock")).unwrap();
        let manifests = [old, new];
        assert!(
            lock.update(&project, &manifests, &[], false)
                .unwrap_err()
                .to_string()
                .contains("MAJOR VERSION CHANGE")
        );
        assert_eq!(std::fs::read(project.join("selu.lock")).unwrap(), before);
        let report = lock.update(&project, &manifests, &[], true).unwrap();
        assert!(report.contains("main.selu:2:"), "{report}");
        assert_eq!(Lock::read(&project).unwrap().targets["wow"], "wow@13.1.5.1");
        std::fs::remove_dir_all(project).unwrap();
    }
    #[test]
    fn locks_survive_new_builds_and_missing_builds_fail_closed() {
        let mut lock = Lock {
            version: 1,
            ..Lock::default()
        };
        let targets = ["wow".to_string()];
        assert_eq!(
            lock.resolve(&targets, &[manifest(1)]).unwrap()[0]
                .build
                .build,
            1
        );
        assert_eq!(lock.discovery_targets(&targets).unwrap(), ["wow@12.1.5.1"]);
        assert_eq!(
            lock.resolve(&targets, &[manifest(1), manifest(2)]).unwrap()[0]
                .build
                .build,
            1
        );
        assert!(
            lock.resolve(&targets, &[manifest(2)])
                .unwrap_err()
                .to_string()
                .contains("fetch")
        );
        assert_eq!(
            lock.resolve(&["wow@12.1.5.2".into()], &[manifest(2)])
                .unwrap()[0]
                .build
                .build,
            2
        );
        assert!(TargetSpec::parse("wow@12.1").is_err());
    }
}
