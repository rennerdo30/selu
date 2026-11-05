use selu_manifest_gen::{merge_system, read_documentation};
#[test]
fn ticker_callback_and_result_are_cancellable_handles() {
    let source = r#"APIDocumentation:AddDocumentationTable({Name="UITimer",Type="System",Namespace="C_Timer",Functions={{Name="NewTicker",Type="Function",Arguments={{Name="seconds",Type="number"},{Name="callback",Type="TickerCallback"},{Name="iterations",Type="number",Nilable=true}},Returns={{Name="cbObject",Type="TickerCallback"}}}},Tables={{Name="TickerCallback",Type="CallbackType",Arguments={{Name="cb",Type="TimerCallback"}}},{Name="TimerCallback",Type="CallbackType"}}})"#;
    let mut m = serde_json::from_str::<selu_manifest::Manifest>(
        r#"{"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#,
    )
    .unwrap();
    for system in read_documentation(source).unwrap() {
        merge_system(&mut m, system);
    }
    assert_eq!(
        m.tables
            .iter()
            .find(|t| t.name == "TickerCallback")
            .unwrap()
            .arguments[0]
            .ty
            .as_deref(),
        Some("SeluTimerHandle")
    );
    assert_eq!(
        m.functions
            .iter()
            .find(|f| f.name == "C_Timer.NewTicker")
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .returns[0]
            .ty
            .as_deref(),
        Some("SeluTimerHandle")
    );
    let program = selu_syntax::parse(
        "ticker.selu",
        "C_Timer.NewTicker(1, () => {}); const handle = C_Timer.NewTicker(1, ticker => { ticker.Cancel(); }, 2); handle.Cancel();",
    )
    .unwrap();
    let checked = selu_check::check_program(&program, &m);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
}
