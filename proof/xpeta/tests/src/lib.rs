#[cfg(test)]
mod tests {
    use selu_sim::Sim;
    fn files() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let source = include_str!("../../src/main.selu");
        let manifests = selu_manifest::load_manifest_dir(&root.join("manifests")).unwrap();
        let program = selu_syntax::parse("main.selu", source).unwrap();
        let mut result = vec![];
        for manifest in manifests.iter().filter(|m| !m.fixture) {
            let checked = selu_check::check_program(&program, manifest);
            assert!(
                !checked.has_errors(),
                "{}: {:?}",
                manifest.target_string(),
                checked.diagnostics
            );
            if manifest.flavour == "retail" {
                let compiled =
                    selu_lower::lower_with_addon(&program, manifest, &checked.types, "XPETAProof");
                let lua = if std::env::var_os("XPETA_BREAK").is_some() {
                    compiled
                        .lua
                        .replace("gains = gains + 1", "gains = gains + 0")
                } else {
                    compiled.lua
                };
                result.push(("main.lua".into(), lua));
            }
        }
        assert_eq!(result.len(), 1);
        result
    }
    fn setup() -> Sim {
        let sim = Sim::new().unwrap();
        sim.eval(r#"
            xp=0; maximum=1000; level=10; disabled=false; locale='enUS'
            function UnitXP() return xp end
            function UnitXPMax() return maximum end
            function UnitLevel() return level end
            function IsXPUserDisabled() return disabled end
            function GetLocale() return locale end
            function time() return 100000 + GetTime() end
            function UnitName() return 'Pilot' end
            function GetRealmName() return 'Realm' end
            RA={db={xpEta={enabled=true}}, RegisterStrings=function() end, Log=function() end}
            function RA.L(_,duration,kills)
                if locale=='deDE' then return string.format('%s bis zum Level (~%d Kills)',duration,kills) end
                return string.format('%s to level (~%d kills)',duration,kills)
            end
            function wipe(t) for k in pairs(t) do t[k]=nil end end
        "#).unwrap();
        sim.load_addon("XPETAProof", &files()).unwrap();
        let reference = format!(
            "(function(...) {}\nRA.record=RecordSample; RA.text=ComputeETAText; RA.load=LoadSamples end)(\"Reference\",RA)",
            include_str!("../original.txt")
        );
        sim.eval(&reference).unwrap();
        sim.fire("PLAYER_LOGIN", vec![]).unwrap();
        sim.eval("RA.record()").unwrap();
        sim
    }
    fn compare(sim: &Sim) {
        sim.eval(r#"
            local region=XPETAProofText
            assert(region:GetText()==(RA.text() or ''), 'differential ETA mismatch: '..tostring(region:GetText())..' vs '..tostring(RA.text()))
        "#).unwrap();
    }
    #[test]
    fn rolling_rate_matches_original() {
        let sim = setup();
        compare(&sim);
        for (delay, xp) in [
            (15.0, 100),
            (15.0, 200),
            (1.0, 300),
            (1.0, 400),
            (15.0, 400),
            (60.0, 500),
        ] {
            sim.advance_time(delay);
            sim.eval(&format!("xp={xp}; RA.record()")).unwrap();
            sim.fire("PLAYER_XP_UPDATE", vec![]).unwrap();
            compare(&sim);
        }
        sim.eval("locale='deDE'; RA.record()").unwrap();
        sim.fire("PLAYER_XP_UPDATE", vec![]).unwrap();
        compare(&sim);
        sim.advance_time(901.0);
        sim.eval("RA.record()").unwrap();
        sim.fire("PLAYER_XP_UPDATE", vec![]).unwrap();
        compare(&sim);
        println!("simulator logs: {:?}", sim.logs());
    }
    #[test]
    fn eligibility_reset_and_commands() {
        let sim = setup();
        sim.advance_time(30.0);
        sim.eval("xp=200").unwrap();
        sim.fire("PLAYER_XP_UPDATE", vec![]).unwrap();
        sim.eval("assert(XPETAProofPanel:IsShown())").unwrap();
        sim.slash("/xpeta").unwrap();
        sim.eval("assert(not XPETAProofPanel:IsShown())").unwrap();
        sim.slash("/xpeta").unwrap();
        sim.eval("disabled=true").unwrap();
        sim.fire("PLAYER_XP_UPDATE", vec![]).unwrap();
        sim.eval("assert(not XPETAProofPanel:IsShown()); disabled=false; xp=0; level=11")
            .unwrap();
        sim.fire("PLAYER_LEVEL_UP", vec![]).unwrap();
        sim.eval("assert(not XPETAProofPanel:IsShown()); assert(XPETAProof_history.level==11); assert(#XPETAProof_history.samples==1)").unwrap();
        sim.slash("/xpeta reset").unwrap();
    }
    #[test]
    fn saved_epoch_history_restores_and_rejects_rollback() {
        let sim = setup();
        sim.advance_time(30.0);
        sim.eval("xp=200").unwrap();
        sim.fire("PLAYER_XP_UPDATE", vec![]).unwrap();
        let snapshot = sim
            .logout_saved(&["XPETAProof_history", "XPETAProof_settings"])
            .unwrap();
        let restored = Sim::login_saved("XPETAProof", &files(), &snapshot).unwrap();
        restored.eval("xp=200; function UnitXP() return xp end; function UnitXPMax() return 1000 end; function UnitLevel() return 10 end; function IsXPUserDisabled() return false end; function time() return 100030+GetTime() end").unwrap();
        restored.fire("PLAYER_LOGIN", vec![]).unwrap();
        restored.eval("assert(XPETAProofPanel:IsShown()); assert(XPETAProofText:GetText()=='~2m to level (~4 kills)')").unwrap();
        let rollback = Sim::login_saved("XPETAProof", &files(), &snapshot).unwrap();
        rollback.eval("function UnitXP() return 1 end; function UnitXPMax() return 1000 end; function UnitLevel() return 10 end; function IsXPUserDisabled() return false end; function time() return 100030+GetTime() end").unwrap();
        rollback.fire("PLAYER_LOGIN", vec![]).unwrap();
        rollback
            .eval("assert(not XPETAProofPanel:IsShown()); assert(#XPETAProof_history.samples==1)")
            .unwrap();
    }
}
