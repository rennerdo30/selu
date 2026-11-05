//! Reviewed pool contracts, applied through the sourced overlay validator.
//! Each overlay records the exact extraction pin and defining source lines.
use selu_manifest::Manifest;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn extend(m: &mut Manifest, sources: &[(String, String)]) -> crate::Result<()> {
    let Some((path, text)) = sources.iter().find(|(p, _)| p.ends_with("/Pools.lua")) else {
        reviewed_file(m, sources, "Pools.lua", &[])?;
        return Ok(());
    };
    // An unreviewed release keeps W2's conservative analysis and all other APIs.
    for (file, digests) in [
        (
            "Pools.lua",
            &[
                "5bfed7b01b3ce92b20cab4ebb17aba18a3c9a1974edc8fa49ca4dd91ca119636",
                "5f3f69be957f18677cf3c9e1ca9d9781d21103597b514cd93fab8fb084f1d888",
            ][..],
        ),
        (
            "SecureTypes.lua",
            &[
                "d1f3ba51e0c888dbf366a438fb28348e54db891d8b75734d97c71950be8a2ad6",
                "e6bf67be9c027a4f74412a1ec90e071c95b2526a7c5bbece5bcb7c133351c291",
            ][..],
        ),
        (
            "ProxyUtil.lua",
            &[
                "14ffb5161841240b9bce0cb934b1e53c8c174c1c9d0c631a79d72993c5205fb7",
                "2acfbfa324592457b7118bb2c782667d2faa7ef963051e08a6f3da79b60de76d",
            ][..],
        ),
    ] {
        if !reviewed_file(m, sources, file, digests)? {
            return Ok(());
        }
    }
    let commit = m.source.as_ref().map(|s| s.commit.as_str()).unwrap_or("");
    let sites = text
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            l.trim().starts_with("function ")
                || l.trim().starts_with("local function ")
                || l.starts_with("Create") && l.contains(" = Create")
        })
        .map(|(i, _)| format!("{path}:{}", i + 1))
        .collect::<Vec<_>>()
        .join("; ");
    let secure = sources
        .iter()
        .find(|(p, _)| p.ends_with("/SecureTypes.lua"))
        .map(|(p, t)| {
            t.lines()
                .enumerate()
                .filter(|(_, l)| l.contains("function "))
                .map(|(i, _)| format!("{p}:{}", i + 1))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    let note = format!(
        "Pinned {commit}: {path}:{sites}; {secure}. Reviewed public pool protocol; callbacks and object elements retain untyped game values."
    );
    let mut widgets = Vec::new();
    // Canonical XML identities for native ModelScene receivers and actors. The
    // generated documentation names the API groups, rather than these XML tags.
    for (name, bases) in [
        ("ModelScene", vec!["FrameAPIModelSceneFrame", "Frame"]),
        (
            "ModelSceneActor",
            vec![
                "FrameAPIModelSceneFrameActor",
                "FrameAPIModelSceneFrameActorBase",
            ],
        ),
    ] {
        if bases
            .iter()
            .any(|base| !m.widgets.iter().any(|w| &w.name == base))
        {
            return Err("missing ModelScene API evidence for pool overlay".into());
        }
        let schema = sources.iter().find(|(p, _)| p.ends_with("/UI.xsd"));
        let schema_note = if let Some((p, t)) = schema {
            t.lines()
                .enumerate()
                .filter(|(_, l)| {
                    l.contains("ModelSceneType")
                        || l.contains("ModelSceneActorType")
                        || l.contains("name=\"Actor\"")
                })
                .map(|(i, _)| format!("{p}:{}", i + 1))
                .collect::<Vec<_>>()
                .join("; ")
        } else {
            return Err("missing ModelScene XML identity evidence".into());
        };
        let api_sites = sources
            .iter()
            .filter(|(p, _)| {
                bases
                    .iter()
                    .any(|b| p.ends_with(&format!("{b}Documentation.lua")))
            })
            .map(|(p, _)| format!("{p}:1"))
            .collect::<Vec<_>>()
            .join("; ");
        widgets.push(json!({"evidence":"source","note":format!("Pinned {commit}: {schema_note}; {api_sites}"),"entry":{"name":name,"type":"ScriptObject","inherits":bases}}));
    }

    let mut functions = Vec::new();
    fn arg(name: &str, ty: &str, optional: bool) -> Value {
        let mut a = json!({"name":name});
        if !ty.is_empty() {
            a["type"] = json!(ty);
        }
        if optional {
            a["attributes"] = json!({"Nilable":true});
        }
        a
    }
    fn method(name: &str, arguments: Vec<Value>, returns: Vec<Value>) -> Value {
        json!({"name":name,"type":"Function","arguments":arguments,"returns":returns})
    }
    let factories: Vec<_> = m
        .functions
        .iter()
        .filter(|f| {
            f.name.starts_with("Create")
                && f.name.contains("Pool")
                && (text.contains(&format!("function {}(", f.name))
                    || text.contains(&format!("{} = Create", f.name)))
        })
        .cloned()
        .collect();
    for mut f in factories {
        let mut binding = f.name.clone();
        for _ in 0..4 {
            if let Some(line) = text
                .lines()
                .find(|l| l.starts_with(&format!("{binding} = Create")))
            {
                binding = line
                    .split('=')
                    .nth(1)
                    .unwrap()
                    .trim()
                    .trim_end_matches(';')
                    .into();
            } else {
                break;
            }
        }
        if let Some(line) = text
            .lines()
            .find(|l| l.starts_with(&format!("function {binding}(")))
        {
            f.params = line
                .split_once('(')
                .unwrap()
                .1
                .split(')')
                .next()
                .unwrap()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
        }
        let collection = f.name.ends_with("PoolCollection");
        let unsecured = f.name.contains("Unsecured");
        let element = if f.name.contains("FontString") {
            "FontString"
        } else if f.name.contains("MaskTexture") {
            "MaskTexture"
        } else if f.name.contains("Texture") {
            "Texture"
        } else if f.name.contains("Frame") {
            "Frame"
        } else if f.name.contains("Actor") {
            "ModelSceneActor"
        } else {
            ""
        };
        // Internal instance constructors are not public factory APIs.
        if f.name.ends_with("Instance") {
            continue;
        }
        let pool_name = format!(
            "W3{}{}Pool",
            if unsecured { "Unsecured" } else { "Secure" },
            if element.is_empty() {
                "Object"
            } else {
                element
            }
        );
        let result_name = if collection {
            format!("W3{}", f.name)
        } else {
            pool_name.clone()
        };
        let variants: Vec<String> = if element == "Frame" && !collection {
            std::iter::once("Frame".into())
                .chain(std::iter::once("ModelScene".into()))
                .chain(
                    m.widgets
                        .iter()
                        .filter(|w| {
                            w.name != "Frame"
                                && w.name != "ModelScene"
                                && is_frame(m, &w.name, &mut std::collections::BTreeSet::new())
                        })
                        .map(|w| w.name.clone()),
                )
                .collect()
        } else {
            vec![element.into()]
        };
        for el in variants {
            let name = if element == "Frame" && !collection {
                format!("{pool_name}_{el}")
            } else {
                result_name.clone()
            };
            let mut methods = vec![
                method(
                    "Acquire",
                    vec![],
                    vec![arg("object", &el, true), arg("new", "bool", false)],
                ),
                method(
                    "Release",
                    vec![
                        arg("object", &el, false),
                        arg("canFailToFindObject", "bool", true),
                    ],
                    vec![arg("active", "bool", false)],
                ),
                method("ReleaseAll", vec![], vec![arg("result", "", false)]),
                method("GetNumActive", vec![], vec![arg("count", "number", false)]),
                method(
                    "DoesObjectBelongToPool",
                    vec![arg("object", &el, false)],
                    vec![arg("belongs", "bool", false)],
                ),
                method(
                    "EnumerateActive",
                    vec![],
                    vec![
                        json!({"name":"iterator","attributes":{"SeluIterator":if collection{json!([el])}else{json!([el,"bool"])}}}),
                    ],
                ),
            ];
            if !collection {
                methods.push(method(
                    "IsActive",
                    vec![arg("object", &el, false)],
                    vec![arg("active", "bool", false)],
                ));
                methods.push(method(
                    "GetNextActive",
                    vec![arg("current", &el, true)],
                    vec![arg("object", &el, true), arg("active", "bool", true)],
                ));
            } else {
                methods[0] = method(
                    "Acquire",
                    vec![
                        arg("template", "string", false),
                        arg("specialization", "", true),
                    ],
                    vec![arg("object", &el, true), arg("new", "bool", false)],
                );
                methods[1] = method(
                    "Release",
                    vec![arg("object", &el, false)],
                    vec![arg("result", "", false)],
                );
                for member in ["CreatePool", "GetOrCreatePool", "GetPool"] {
                    let args = if member == "GetPool" {
                        vec![
                            arg("template", "string", false),
                            arg("specialization", "", true),
                        ]
                    } else if element == "Frame" {
                        vec![
                            arg("frameType", "string", false),
                            arg("parent", "Frame", true),
                            arg("template", "string", false),
                            arg("resetFunc", "LuaFunctionReference", true),
                            arg("forbidden", "bool", true),
                            arg("specialization", "", true),
                            arg("capacity", "number", true),
                        ]
                    } else {
                        vec![
                            arg("parent", "Frame", false),
                            arg("layer", "string", true),
                            arg("subLayer", "number", true),
                            arg("template", "string", false),
                            arg("resetFunc", "LuaFunctionReference", true),
                            arg("capacity", "number", true),
                        ]
                    };
                    let target = if element == "Frame" {
                        format!("{pool_name}_Frame")
                    } else {
                        pool_name.clone()
                    };
                    let mut d = method(
                        member,
                        args,
                        vec![arg("pool", &target, member == "GetPool")],
                    );
                    if element == "Frame" && member != "GetPool" {
                        d["attributes"] = json!({"SeluPoolFactory":true});
                    }
                    methods.push(d);
                }
            }
            if !unsecured {
                // SecureStack.Contains forwards the engine tContains helper, whose
                // return declaration is absent at these pins. Do not invent bool.
                if let Some(d) = methods
                    .iter_mut()
                    .find(|d| d["name"] == "DoesObjectBelongToPool")
                {
                    d["returns"] = json!([arg("belongs", "", false)]);
                }
            }
            if !text.contains("function ObjectPoolMixin:DoesObjectBelongToPool(")
                || (collection && !unsecured)
            {
                methods.retain(|d| d["name"] != "DoesObjectBelongToPool");
            }
            // Proxy GetOrCreatePool returns only the pool; the unsecured implementation also returns new.
            if collection
                && unsecured
                && let Some(d) = methods.iter_mut().find(|d| d["name"] == "GetOrCreatePool")
            {
                d["returns"]
                    .as_array_mut()
                    .unwrap()
                    .push(arg("new", "bool", false));
            }
            widgets.push(json!({"evidence":"source","note":note,"entry":{"name":name,"type":"ScriptObject","attributes":{"SeluPoolEvidence":note},"methods":methods}}));
        }
        let mut args = f
            .params
            .iter()
            .map(|n| {
                arg(
                    n,
                    match n.as_str() {
                        "frameType" | "template" | "layer" => "string",
                        "parent" => {
                            if element == "ModelSceneActor" {
                                "ModelScene"
                            } else {
                                "Frame"
                            }
                        }
                        "subLayer" | "capacity" => "number",
                        "forbidden" => "bool",
                        "createFunc" | "resetFunc" | "postCreate" => "LuaFunctionReference",
                        _ => "",
                    },
                    n != "frameType"
                        && n != "createFunc"
                        && (n != "parent" || element == "Frame")
                        && !(unsecured && element.is_empty() && n == "resetFunc"),
                )
            })
            .collect::<Vec<_>>();
        if collection {
            args.clear();
        }
        let target = if element == "Frame" && !collection {
            format!("{pool_name}_Frame")
        } else {
            result_name
        };
        let signature = json!({"name":f.name,"type":"Function","arguments":args,"returns":[arg("pool",&target,false)],"attributes":{"SeluPoolFactory":element=="Frame" && !collection}});
        functions.push(json!({"evidence":"source","note":note,"entry":{"name":f.name,"params":f.params,"evidence":"source","signature":signature}}));
    }
    let overlay = json!({"widgets":widgets,"functions":functions});
    crate::apply_overlay(m, &toml::to_string(&overlay)?)
}

fn is_frame(m: &Manifest, name: &str, seen: &mut std::collections::BTreeSet<String>) -> bool {
    if name == "Frame" {
        return true;
    }
    if !seen.insert(name.into()) {
        return false;
    }
    m.widgets.iter().find(|w| w.name == name).is_some_and(|w| {
        w.inherits.iter().any(|base| is_frame(m, base, seen))
            || w.attributes
                .get("SeluAliasOf")
                .and_then(|v| v.as_str())
                .is_some_and(|base| is_frame(m, base, seen))
    })
}

/// The updater uses the same rendering for newly extracted and retained builds.
pub fn warning_summary(m: &Manifest) -> String {
    m.source
        .as_ref()
        .map(|s| {
            s.warnings
                .iter()
                .map(|w| format!("WARNING: {w}\n"))
                .collect()
        })
        .unwrap_or_default()
}

fn public_pool_factory(name: &str) -> bool {
    let Some(kind) = name.strip_prefix("Create") else {
        return false;
    };
    let kind = kind
        .strip_prefix("Unsecured")
        .or_else(|| kind.strip_prefix("Secure"))
        .unwrap_or(kind);
    [
        "Frame",
        "Texture",
        "FontString",
        "MaskTexture",
        "Object",
        "Actor",
    ]
    .iter()
    .any(|base| kind == format!("{base}Pool") || kind == format!("{base}PoolCollection"))
}

fn reviewed_file(
    m: &mut Manifest,
    sources: &[(String, String)],
    file: &str,
    reviewed: &[&str],
) -> crate::Result<bool> {
    let text = sources
        .iter()
        .find(|(path, _)| path.ends_with(&format!("/{file}")))
        .map(|(_, text)| text);
    let digest = text.map(|text| {
        Sha256::digest(text.replace("\r\n", "\n").as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    });
    if digest
        .as_ref()
        .is_some_and(|hash| reviewed.contains(&hash.as_str()))
    {
        return Ok(true);
    }
    let reason = if text.is_some() {
        "changed since the reviewed versions"
    } else {
        "is missing from this release"
    };
    let warning = format!(
        "pool contracts withheld for {}: {file} {reason}; review and add its digest",
        m.target_string()
    );
    // Fresh extraction has no W3 declarations yet. Also remove a previously
    // applied overlay if a caller is reusing a manifest; never retain stale proof.
    let constructors: std::collections::BTreeSet<_> = m
        .functions
        .iter()
        .filter(|f| {
            public_pool_factory(&f.name)
                || (f.name.starts_with("Create")
                    && f.name.contains("Pool")
                    && f.provenance
                        .as_ref()
                        .is_some_and(|s| s.file.ends_with("/Pools.lua")))
        })
        .map(|f| f.name.clone())
        .collect();
    let pool_types: std::collections::BTreeSet<_> = m
        .widgets
        .iter()
        .filter(|w| {
            w.attributes.contains_key("SeluPoolEvidence")
                || w.name
                    .strip_prefix("SourceObject_")
                    .is_some_and(|name| constructors.contains(name))
        })
        .map(|w| w.name.clone())
        .collect();
    for f in &mut m.functions {
        if constructors.contains(&f.name)
            || f.signature.as_ref().is_some_and(|d| {
                d.returns
                    .iter()
                    .any(|r| r.ty.as_ref().is_some_and(|ty| pool_types.contains(ty)))
            })
        {
            f.signature = None;
        }
    }
    m.widgets.retain(|w| !pool_types.contains(&w.name));
    let source = m
        .source
        .as_mut()
        .ok_or("pool withholding requires build provenance")?;
    if !source.warnings.contains(&warning) {
        source.warnings.push(warning);
    }
    Ok(false)
}

#[cfg(test)]
mod withholding_tests {
    use super::*;
    #[test]
    fn changed_and_missing_helpers_withhold_without_errors() {
        for file in ["SecureTypes.lua", "ProxyUtil.lua"] {
            let mut m:Manifest=serde_json::from_value(json!({"flavour":"wow","build":{"major":12,"minor":1,"patch":5,"build":1},"interface":120105,"functions":[{"name":"CreateFramePool","evidence":"source","source":{"file":"Other/ObjectPool.lua","kind":"defined"},"signature":{"name":"CreateFramePool","type":"Function","returns":[{"name":"pool","type":"SourceObject_CreateFramePool"}]}}],"widgets":[{"name":"SourceObject_CreateFramePool","type":"ScriptObject","methods":[{"name":"Acquire","type":"Function"}]}],"source":{"commit":"fixture","version":"12.1.5","interfaceSource":"fixture"}})).unwrap();
            let sources = vec![(
                format!("Interface/AddOns/Blizzard_SharedXMLBase/{file}"),
                "changed helper".into(),
            )];
            assert!(!reviewed_file(&mut m, &sources, file, &[]).unwrap());
            assert!(m.function("CreateFramePool").unwrap().signature.is_none());
            assert!(
                !m.widgets
                    .iter()
                    .any(|w| w.name == "SourceObject_CreateFramePool")
            );
            assert!(warning_summary(&m).contains(&format!(
                "WARNING: pool contracts withheld for wow@12.1.5.1: {file} changed"
            )));
            assert!(!reviewed_file(&mut m, &[], file, &[]).unwrap());
            assert!(warning_summary(&m).contains("is missing from this release"));
        }
    }
}
