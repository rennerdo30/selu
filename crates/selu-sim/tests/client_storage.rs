use selu_sim::{Session, SessionConfig};

#[test]
#[ignore = "requires owner's Forever install"]
fn client_session_atlas_and_probes() {
    let session = Session::new(SessionConfig {
        target: Some("wowf@1.60.1.70790".into()),
        client_install: Some("E:/Games/Battle.net/World of Warcraft".into()),
        ..Default::default()
    })
    .unwrap();
    session
        .eval(
            r#"
        t=UIParent:CreateTexture(); t:SetSize(25,30); t:SetPoint('CENTER');
        t:SetTexture('Interface/Buttons/WHITE8X8'); assert(t:GetTexture() ~= nil);
        t:SetTexture('Interface/RennersAddons/ProbeMissingTexture'); assert(t:GetTexture() == nil);
        t:SetAtlas('UI-Character-Info-Title'); assert(t:GetWidth()==25 and t:GetHeight()==30);
        t:SetAtlas('UI-Character-Info-Title',true); assert(t:GetWidth()==196 and t:GetHeight()==40);
        t:SetTexture(1400895); assert(t:GetTextureFileID()==1400895);
        t:SetAtlas('UI-Character-Info-Title',true);
    "#,
        )
        .unwrap();
    let snap = session.snapshot().unwrap();
    let texture = snap.regions.iter().find(|r| r.widget == "Texture").unwrap();
    assert!(texture.visual.uv[0] > 0.0);
    assert!(texture.visual.uv[1] < 1.0);
}
