use selu_sim::{Sim, TextureFill};
#[test]
fn toolkit_white_base_then_gradient() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"local t=UIParent:CreateTexture();t:SetColorTexture(1,1,1,1)
        assert(pcall(t.SetGradient,t,'VERTICAL',CreateColor(.1,.2,.3,.4),CreateColor(.5,.6,.7,.8)))
        local c=CreateColor(1,.5,0);assert(c:GenerateHexColorNoAlpha()=='FF8000')
        c:SetRGB(0,0,0);assert(c.a==1 and c:IsEqualTo(CreateColor(0,0,0)))"#,
    )
    .unwrap();
    assert!(matches!(
        sim.snapshot().unwrap().regions[1].texture,
        Some(TextureFill::Gradient { vertical: true, .. })
    ));
}
#[test]
fn copy_buffer_and_alpha_clock() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"e=CreateFrame('EditBox');e:SetSize(300,100);e:SetPoint('CENTER');e:SetMultiLine(true);e:SetTextInsets(4,5,6,7);e:SetText('entry one\nentry two\nentry three')
        g=e:CreateAnimationGroup();a=g:CreateAnimation('Alpha');a:SetDuration(1);a:SetFromAlpha(0);a:SetToAlpha(1);g:Play()"#).unwrap();
    sim.advance_time(0.5);
    sim.eval("assert(g:IsPlaying());assert(e:GetAlpha()>.4 and e:GetAlpha()<.6);g:Pause();assert(g:IsPaused())").unwrap();
    let snap = sim.snapshot().unwrap();
    let e = &snap.regions[1];
    assert_eq!(e.visual.text_insets, [4., 5., 6., 7.]);
    assert!(e.text_layout.as_ref().unwrap().height > 20.);
}
#[test]
fn resolver_classifies_and_caches() {
    use selu_sim::assets::{Lookup, Resolver};
    let resolver = Resolver::default();
    assert!(matches!(
        resolver.lookup("Interface\\Buttons\\WHITE8X8"),
        Lookup::Unavailable
    ));
    assert!(matches!(resolver.lookup("../escape"), Lookup::Missing));
    assert!(matches!(
        resolver.lookup("Media/missing.png"),
        Lookup::Missing
    ));
}
#[test]
fn animation_orders_loops_scripts_and_stop_restore() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
        t=CreateFrame('Frame');t:SetAlpha(.75);g=t:CreateAnimationGroup();played=0;loops=0;finished=0
        g:SetScript('OnPlay',function() played=played+1 end)
        g:SetScript('OnLoop',function() loops=loops+1 end)
        g:SetScript('OnFinished',function() finished=finished+1 end)
        local a=g:CreateAnimation('Alpha');a:SetDuration(.25);a:SetFromAlpha(0);a:SetToAlpha(1)
        local b=g:CreateAnimation('Alpha');b:SetOrder(2);b:SetStartDelay(.1);b:SetDuration(.25);b:SetFromAlpha(1);b:SetToAlpha(0)
        b:SetSmoothing('IN_OUT');g:SetLooping('BOUNCE');g:Play()
    "#).unwrap();
    sim.advance_time(0.7);
    sim.eval("assert(played==1 and loops==1 and finished==0);g:Stop();assert(t:GetAlpha()==.75);g:SetLooping('NONE');g:Play()").unwrap();
    sim.advance_time(0.7);
    sim.eval("assert(finished==1 and not g:IsPlaying() and t:GetAlpha()==.75)")
        .unwrap();
}
#[test]
fn incremental_anchor_dependants_and_live_font_invalidation() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
        a=CreateFrame('Frame');a:SetSize(50,50);a:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',10,10)
        b=CreateFrame('Frame');b:SetSize(20,20);b:SetPoint('LEFT',a,'RIGHT')
        font=CreateFont('SIM3Font');font:SetFont('missing.ttf',12,'')
        label=b:CreateFontString();label:SetFontObject(font);label:SetText('dependent');label:SetPoint('LEFT')
    "#).unwrap();
    let before = sim.snapshot().unwrap();
    sim.eval("a:SetWidth(100);font:SetFont('missing.ttf',24,'')")
        .unwrap();
    let after = sim.snapshot().unwrap();
    assert_eq!(
        after.regions[2].rect.unwrap().left - before.regions[2].rect.unwrap().left,
        50.
    );
    assert!(after.regions[3].rect.unwrap().width > before.regions[3].rect.unwrap().width * 1.9);
}
#[test]
fn addon_resolver_found_missing_and_normalized_cache() {
    use selu_sim::assets::{Lookup, Resolver};
    let root = std::env::temp_dir().join(format!("selu-sim3-assets-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("probe.tga");
    std::fs::write(&file, b"asset bytes").unwrap();
    let mut resolver = Resolver::default();
    resolver.add_addon(root.canonicalize().unwrap());
    assert!(
        matches!(resolver.lookup("probe.tga"),Lookup::Found(bytes) if bytes.as_ref()==b"asset bytes")
    );
    std::fs::remove_file(file).unwrap();
    assert!(matches!(resolver.lookup("PROBE.TGA"), Lookup::Found(_)));
    assert!(matches!(resolver.lookup("missing.tga"), Lookup::Missing));
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn invalid_texture_probe_does_not_echo_requested_path() {
    // https://warcraft.wiki.gg/wiki/API:TextureBase_SetTexture: return is always true.
    // https://warcraft.wiki.gg/wiki/API:TextureBase_GetTexture: invalid input returns nil.
    let sim = Sim::new().unwrap();
    sim.eval("local t=UIParent:CreateTexture();t:SetColorTexture(1,1,1,1);assert(t:SetTexture('missing.tga')==true);assert(t:GetTexture()==nil);assert(t:GetTextureFileID()==nil);t:SetTexture('Interface/Buttons/WHITE8X8');assert(t:GetTexture()==nil)").unwrap();
}
#[test]
fn existing_addon_media_probe_succeeds() {
    use selu_sim::{Session, SessionConfig};
    let name = format!("SIM3Media{}", std::process::id());
    let root = std::env::temp_dir().join(&name);
    std::fs::create_dir_all(&root).unwrap();
    let toc = root.join(format!("{name}.toc"));
    std::fs::write(&toc, "## Interface: 16001\n").unwrap();
    let file = root.join("probe.tga");
    std::fs::write(&file, b"asset bytes").unwrap();
    let mut session = Session::new(SessionConfig::default()).unwrap();
    session.load_addon_dir(&root).unwrap();
    session.eval("local t=UIParent:CreateTexture();assert(t:SetTexture('probe.tga')==true);assert(t:GetTexture()~=nil);t:SetTexture('missing.tga');assert(t:GetTexture()==nil)").unwrap();
    std::fs::remove_file(toc).unwrap();
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(root).unwrap();
}
#[test]
fn translation_changes_paint_without_reflowing_anchors() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
        t=CreateFrame('Frame');t:SetPoint('BOTTOMLEFT',UIParent,'BOTTOMLEFT',20,20);t:SetSize(10,10)
        local g=t:CreateAnimationGroup();local a=g:CreateAnimation('Translation');a:SetDuration(1);a:SetOffset(100,0);g:Play()
    "#).unwrap();
    sim.advance_time(0.5);
    let snap = sim.snapshot().unwrap();
    assert!((snap.regions[1].rect.unwrap().left - 70.).abs() < 2.);
    sim.eval("assert(t:GetLeft()==20)").unwrap();
}

#[test]
fn multiline_scroll_child_has_intrinsic_height() {
    let sim = Sim::new().unwrap();
    sim.eval("local s=CreateFrame('ScrollFrame');s:SetSize(200,100);s:SetPoint('CENTER');e=CreateFrame('EditBox',nil,s);e:SetWidth(200);e:SetMultiLine(true);e:SetTextInsets(8,8,0,0);e:SetPoint('TOPLEFT');e:SetText(string.rep('copy buffer line\\n',31));s:SetScrollChild(e)").unwrap();
    let snap = sim.snapshot().unwrap();
    assert!(snap.regions[2].rect.unwrap().height > 100.);
    assert!(snap.regions[2].clip.is_some());
}
