//! Hooks on Blizzard mixins and game tables, typed hook parameters, the
//! documentation's "any Lua value" as `unknown`, and documented structures
//! as written types (real wowf game data).
use selu_check::{CheckOptions, ModuleBindings, check_module_targets_with_options};
use selu_manifest::Manifest;

fn manifest() -> Manifest {
    selu_manifest::store::load_build(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/store"),
        "wowf@1.60.1.70790",
    )
    .unwrap()
}
fn check(src: &str, m: &Manifest) -> (selu_syntax::Program, selu_check::Checked) {
    let p = selu_syntax::parse("hooks.selu", src).unwrap();
    let c = check_module_targets_with_options(
        &p,
        m,
        ModuleBindings::default(),
        std::slice::from_ref(m),
        CheckOptions::default(),
    )
    .0;
    (p, c)
}
fn codes(src: &str, m: &Manifest) -> Vec<String> {
    check(src, m)
        .1
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}
fn run(src: &str, m: &Manifest, setup: &str, after: &str) -> Vec<String> {
    let (p, c) = check(src, m);
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let lua = selu_lower::lower(&p, m, &c.types).lua;
    let sim = selu_sim::Sim::new().unwrap();
    sim.eval(setup).unwrap();
    sim.load_files(&[("t.lua".into(), lua.clone())])
        .unwrap_or_else(|e| panic!("{e}\n{lua}"));
    sim.eval(after).unwrap_or_else(|e| panic!("{e}\n{lua}"));
    sim.prints()
}

const ROWS: &str = r#"
    allowTaint("reads the row's recipe", hook(ProfessionsRecipeListRecipeMixin, "Init", (row, node, _hide) => {
        const recipe = node.GetData().recipeInfo;
        print(recipe != null ? readable(recipe.recipeID) ?? 0 : -1);
        row.LockedIcon.Hide();
    }));
"#;

#[test]
fn a_mixin_hook_sees_typed_frames_and_arguments() {
    let m = manifest();
    let setup = "ProfessionsRecipeListRecipeMixin = { Init = function() end }";
    let after = "local row = { LockedIcon = { Hide = function() print('hidden') end } }
        ProfessionsRecipeListRecipeMixin.Init(row, { GetData = function() return { recipeInfo = { recipeID = 7 } } end })
        ProfessionsRecipeListRecipeMixin.Init(row, { GetData = function() return {} end })";
    assert_eq!(run(ROWS, &m, setup, after), ["7", "hidden", "-1", "hidden"]);
    // The row is the recipe row button; the node is typed.
    assert_eq!(
        codes(
            r#"allowTaint("t", hook(ProfessionsRecipeListRecipeMixin, "Init", (row, node) => { const n: number = row; }));"#,
            &m
        ),
        ["SELU0336"]
    );
    assert_eq!(
        codes(
            r#"allowTaint("t", hook(ProfessionsRecipeListRecipeMixin, "Init", (row, node) => { const n: number = node.GetData(); }));"#,
            &m
        ),
        ["SELU0336"]
    );
    // A method the game data does not declare is refused, as a call is.
    assert_eq!(
        codes(
            r#"allowTaint("t", hook(ProfessionsRecipeListRecipeMixin, "NoSuchMethod", () => {}));"#,
            &m
        ),
        ["SELU0582"]
    );
    // Unreviewed, a mixin hook is a game hook.
    assert_eq!(
        codes(
            r#"hook(ProfessionsRecipeListRecipeMixin, "Init", () => {});"#,
            &m
        ),
        ["SELU0604"]
    );
}

#[test]
fn a_dotted_hook_hooks_the_game_tables_function() {
    let m = manifest();
    let src = r#"
        allowTaint("reads the recipe", hook("Professions.SetupOutputIcon", (icon, transaction, output) => {
            print(readable(transaction.GetRecipeID()) ?? 0);
            print(output != null ? readable(output.hyperlink) ?? "none" : "nil");
        }));
    "#;
    let setup = "Professions = { SetupOutputIcon = function() end }";
    let after = "Professions.SetupOutputIcon({}, { GetRecipeID = function() return 5 end }, { hyperlink = 'link' })";
    assert_eq!(run(src, &m, setup, after), ["5", "link"]);
}

#[test]
fn widget_method_hooks_receive_the_widget_and_typed_arguments() {
    let m = manifest();
    assert!(
        codes(
            r#"allowTaint("t", hook(UIParent, "SetAlpha", (self, alpha) => { const a: number = alpha; const f: Frame = self; }));"#,
            &m
        )
        .is_empty()
    );
    assert_eq!(
        codes(
            r#"allowTaint("t", hook(UIParent, "SetAlpha", (self, alpha) => { const s: string = alpha; }));"#,
            &m
        ),
        ["SELU0336"]
    );
}

#[test]
fn any_lua_value_is_unknown_and_tables_of_it_read_protected_fields() {
    let m = manifest();
    assert_eq!(
        codes(
            r#"const s = C_Item.GetItemStats("x"); const n: number = s;"#,
            &m
        ),
        ["SELU0336"]
    );
    let src = r#"
        const s = C_Item.GetItemStats("x");
        if (typeof s == "table") {
            const v = s.ITEM_MOD_STAMINA_SHORT;
            if (typeof v == "number") { print(v); }
            print(typeof s.MISSING);
        }
    "#;
    let setup = "C_Item = C_Item or {}; C_Item.GetItemStats = function() return { ITEM_MOD_STAMINA_SHORT = 4 } end";
    assert_eq!(run(src, &m, setup, ""), ["4", "nil"]);
}

#[test]
fn documented_structures_and_mixin_values_are_written_types() {
    let m = manifest();
    assert!(
        codes(
            r#"function id(recipe: TradeSkillRecipeInfo?): number { return recipe != null ? readable(recipe.recipeID) ?? 0 : 0; }
               function red(color: colorRGBA): number { const [r] = color.GetRGB(); return readable(r) ?? 0; }"#,
            &m
        )
        .is_empty()
    );
    assert_eq!(
        codes(r#"const recipe: TradeSkillRecipeInfo = 1;"#, &m),
        ["SELU0336"]
    );
}

#[test]
fn quest_reward_buttons_carry_their_reward_type() {
    let m = manifest();
    assert!(
        codes(
            r#"for (const button of QuestInfoRewardsFrame.RewardButtons) { const kind: string? = button.type; }
               const link: string? = GetQuestItemLink("choice", 1);
               const count: number = GetNumQuestChoices();
               const worn: string? = GetInventoryItemLink("player", 1);"#,
            &m
        )
        .is_empty()
    );
    assert_eq!(
        codes(
            r#"const worn: number = GetInventoryItemLink("player", 1);"#,
            &m
        ),
        ["SELU0336"]
    );
}
