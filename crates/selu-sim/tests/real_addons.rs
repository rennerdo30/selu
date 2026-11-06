use selu_sim::{Session, SessionConfig, Sim};
#[test]
fn missing_toc_file_keeps_loading_and_names_path() {
    let mut session = Session::new(SessionConfig::default()).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/addons/MissingFile");
    session.load_addon_dir(&dir).unwrap();
    assert!(
        session
            .logs()
            .iter()
            .any(|s| s.contains("missing.lua") && s.contains("continuing"))
    );
    session.slash("/missingfile").unwrap();
    assert!(
        session
            .prints()
            .iter()
            .any(|s| s.contains("loaded after missing"))
    );
}
#[test]
fn unknown_global_is_not_callable_and_warns_once() {
    let sim = Sim::new().unwrap();
    sim.eval("assert(UnknownShape == nil); assert(UnknownShape == nil)")
        .unwrap();
    assert_eq!(
        sim.logs()
            .iter()
            .filter(|s| s.contains("UnknownShape"))
            .count(),
        1
    );
}
#[test]
fn font_popup_clock_and_stack_are_stateful_and_deterministic() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
        local f = CreateFont('TestFont'); f:SetFont('test.ttf', 17, 'OUTLINE')
        local p,s,flags = f:GetFont(); assert(p=='test.ttf' and s==17 and flags=='OUTLINE')
        assert(debugprofilestop() == 0)
        StaticPopupDialogs.TEST = {text='Hello %s',button1='OK',button2='Cancel'}
        for i=1,20 do StaticPopup_Show('TEST','world') end
        assert(#__selu_popups==16 and __selu_popups[16].text=='Hello world')
        local function capture() return debugstack(1,12,10) end
        local a,b=capture(),capture(); assert(a==b and a:find('in function'))
        local co=coroutine.create(function() local stack=debugstack(); assert(stack:find('in function')) end)
        assert(coroutine.resume(co))
    "#).unwrap();
    sim.advance_time(0.25);
    sim.eval("assert(debugprofilestop()==250)").unwrap();
}
#[test]
fn manifest_shapes_are_not_functions() {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../manifests/fixtures/forever-1.0.0.1000.json"
    ))
    .unwrap();
    value["globals"] = serde_json::json!([
        {"name":"GameFontNormal", "type":"Font", "evidence":"verified"},
        {"name":"NativeTable", "type":"table", "evidence":"verified"},
        {"name":"NativeFrame", "type":"Frame", "evidence":"verified"}
    ]);
    value["widgets"] = serde_json::json!([{"name":"Font","evidence":"verified"}]);
    let manifest = serde_json::from_value(value).unwrap();
    let session = Session::new(SessionConfig {
        manifest: Some(manifest),
        ..Default::default()
    })
    .unwrap();
    session.eval("assert(type(GameFontNormal)=='table' and GameFontNormal.GetFont); assert(type(NativeTable)=='table'); assert(NativeFrame:GetObjectType()=='Frame'); local text=UIParent:CreateFontString(nil,'OVERLAY','GameFontNormal');assert(text:GetFontObject()==GameFontNormal)").unwrap();
}
#[test]
fn screen_scale_and_unknown_addon_fields() {
    let session = Session::new(SessionConfig {
        ui_size: (1280.0, 720.0),
        ..Default::default()
    })
    .unwrap();
    session.eval(r#"
        local w,h=GetPhysicalScreenSize(); assert(w==1280 and h==720)
        assert(UIParent:GetScale()==1 and GetScreenWidth()==1280)
        assert(CreateFrame('Frame').RA_kitOriginalScale==nil)
        UIParent:SetScale(.5)
        assert(UIParent:GetEffectiveScale()==.5 and UIParent:GetWidth()==2560)
        assert(GetScreenWidth()==2560 and GetScreenHeight()==1440)
        local child=CreateFrame('Frame'); child:SetScale(.5); assert(child:GetEffectiveScale()==.25)
        for _,scale in ipairs({0,-1,0/0,math.huge}) do assert(not pcall(UIParent.SetScale,UIParent,scale)) end
        assert(UIParent:GetScale()==.5)
    "#).unwrap();
}
