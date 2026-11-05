use ab_glyph::Font;
use selu_sim::{Session, SessionConfig, Sim, core::VisualStyle, fonts::FontBook, text::Content};

fn style() -> VisualStyle {
    VisualStyle {
        size: 20.0,
        ..Default::default()
    }
}
fn reference(face: &impl Font, text: &str, size: f32) -> f64 {
    let font = harfrust::FontRef::new(face.font_data()).unwrap();
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let data = harfrust::ShaperData::new(&font);
    let shaped = data.shaper(&font).build().shape(buffer, Default::default());
    let units: i32 = shaped.glyph_positions().iter().map(|p| p.x_advance).sum();
    (units as f32 * size / face.units_per_em().unwrap()) as f64
}
#[test]
fn kerning_baselines_and_snapshot_use_font_tables() {
    let face = ab_glyph::FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).unwrap();
    let expected = reference(&face, "AVATAR", 20.0);
    let unkerned: f32 = "AVATAR"
        .chars()
        .map(|c| face.h_advance_unscaled(face.glyph_id(c)))
        .sum();
    assert!(expected < (unkerned * 20.0 / face.units_per_em().unwrap()) as f64);
    let sim = Sim::new().unwrap();
    sim.eval("T=UIParent:CreateFontString(); T:SetFont('Fonts/FRIZQT__.TTF',20); T:SetText('AVATAR'); T:SetPoint('TOPLEFT'); print(T:GetStringWidth(),T:GetStringHeight(),T:GetHeight())").unwrap();
    let snap = sim.snapshot().unwrap();
    let t = snap.regions.last().unwrap();
    let shaped = t.text_layout.as_ref().unwrap();
    assert!((shaped.width - expected).abs() < 1e-5);
    assert!((t.rect.unwrap().width - expected).abs() < 1e-5);
    let last = shaped.placements.last().unwrap();
    let Content::Glyph(id) = last.content else {
        panic!("glyph")
    };
    let advance =
        face.h_advance_unscaled(ab_glyph::GlyphId(id)) * 20.0 / face.units_per_em().unwrap();
    assert!((last.x as f64 + advance as f64 - expected).abs() < 1e-5);
    let row = (face.ascent_unscaled() - face.descent_unscaled() + face.line_gap_unscaled()) * 20.0
        / face.units_per_em().unwrap();
    assert!((shaped.height - row as f64).abs() < 1e-5);
    assert!(shaped.placements.iter().all(|p| {
        (p.y - face.ascent_unscaled() * 20.0 / face.units_per_em().unwrap()).abs() < 1e-5
    }));
    assert!(!shaped.masks.is_empty());
    let printed: Vec<f64> = sim.prints()[0]
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    for (actual, want) in printed.iter().zip([expected, shaped.height, shaped.height]) {
        assert!((actual - want).abs() < 1e-4);
    }
}
#[test]
fn word_boundaries_long_words_flags_and_ellipsis() {
    let book = FontBook::default();
    let mut s = style();
    let limit = book.layout("one two", &s, None, None).width + 0.01;
    let wrapped = book.layout("one two three", &s, Some(limit), None);
    assert_eq!(wrapped.lines, 2);
    let first_baseline = wrapped.placements[0].y;
    // The first row is exactly "one two"; no part of "three" leaks onto it.
    assert_eq!(
        wrapped
            .placements
            .iter()
            .filter(|p| p.y == first_baseline)
            .count(),
        7
    );
    let long = book.layout("WWWWWWWW", &s, Some(limit), None);
    assert!(long.lines > 1 && !long.truncated);
    s.non_space_wrap = false;
    let cut = book.layout("WWWWWWWW", &s, Some(limit), None);
    assert_eq!(cut.lines, 1);
    assert!(cut.truncated && cut.width <= limit);
    let ellipsis = book.face("").glyph_id('…').0;
    assert_eq!(
        cut.placements.last().unwrap().content,
        Content::Glyph(ellipsis)
    );
    s.wrap = false;
    s.non_space_wrap = true;
    assert_eq!(book.layout("one two three", &s, Some(limit), None).lines, 1);
    s.wrap = true;
    s.max_lines = 1;
    assert!(
        book.layout("one two three", &s, Some(limit), None)
            .truncated
    );
    s.ellipsis = false;
    assert_ne!(
        book.layout("one two three", &s, Some(limit), None)
            .placements
            .last()
            .unwrap()
            .content,
        Content::Glyph(ellipsis)
    );
    let sim = Sim::new().unwrap();
    sim.eval("local t=UIParent:CreateFontString(); t:SetFont('fallback',20);t:SetWidth(45);t:SetText('one two three');t:SetMaxLines(1);t:SetNonSpaceWrap(false);t:SetWordWrap(false);assert(not t:CanWordWrap() and not t:CanNonSpaceWrap());assert(t:IsTruncated());assert(t:GetMaxLines()==1)").unwrap();
}
#[test]
fn markup_icons_spacing_and_template_inheritance() {
    let sim = Sim::new().unwrap();
    sim.eval("local f=CreateFont('FixtureFont');f:SetFont('fallback',20,'OUTLINE');f:SetTextColor(.2,.4,.6,1); T=UIParent:CreateFontString(nil,'OVERLAY','FixtureFont');T:SetText('A|cffff0000V|r |Tchecker.tga:24:30|t');T:SetPoint('TOPLEFT');T:SetSpacing(3);assert(select(2,T:GetFont())==20);assert(T:GetSpacing()==3)").unwrap();
    let snap = sim.snapshot().unwrap();
    let t = snap.regions.last().unwrap();
    assert_eq!(t.visual.color, [0.2, 0.4, 0.6, 1.0]);
    let shaped = t.text_layout.as_ref().unwrap();
    assert_eq!(shaped.placements[1].color, [1.0, 0.0, 0.0, 1.0]);
    let Content::Texture { width, height, .. } = shaped.placements.last().unwrap().content else {
        panic!("inline texture")
    };
    assert_eq!((width, height), (30.0, 24.0));
    let font = FontBook::default();
    let plain = font.layout("AV", &style(), None, None);
    assert!(shaped.width > plain.width + 29.0);
}
#[test]
fn addon_font_is_sandboxed_and_reregistered_on_reload() {
    let root = std::env::temp_dir().join(format!("selu-text-font-{}", std::process::id()));
    assert!(!root.exists());
    std::fs::create_dir(&root).unwrap();
    let addon = root.join("FontFixture");
    std::fs::create_dir(&addon).unwrap();
    std::fs::write(addon.join("face.ttf"), epaint_default_fonts::HACK_REGULAR).unwrap();
    std::fs::write(
        addon.join("FontFixture.toc"),
        "## Interface: 11500\nmain.lua\n",
    )
    .unwrap();
    std::fs::write(addon.join("main.lua"),"T=UIParent:CreateFontString();T:SetFont('Interface\\\\AddOns\\\\FontFixture\\\\face.ttf',20);T:SetText('iiii');T:SetPoint('TOPLEFT')").unwrap();
    let mut session = Session::new(SessionConfig::default()).unwrap();
    session.load_addon_dir(&addon).unwrap();
    let own = ab_glyph::FontRef::try_from_slice(epaint_default_fonts::HACK_REGULAR).unwrap();
    let expected = reference(&own, "iiii", 20.0);
    let before = session.snapshot().unwrap();
    assert!(
        (before
            .regions
            .last()
            .unwrap()
            .text_layout
            .as_ref()
            .unwrap()
            .width
            - expected)
            .abs()
            < 1e-5
    );
    session.reload().unwrap();
    let after = session.snapshot().unwrap();
    assert_eq!(
        before.regions.last().unwrap().text_layout,
        after.regions.last().unwrap().text_layout
    );
    let roots = [addon.canonicalize().unwrap()];
    assert!(selu_sim::fonts::resolve(&roots, "../face.ttf").is_err());
    assert!(selu_sim::fonts::resolve(&roots, "Interface/AddOns/Other/face.ttf").is_err());
    assert!(selu_sim::fonts::resolve(&roots, root.join("outside.ttf").to_str().unwrap()).is_err());
    drop(session);
    std::fs::remove_file(addon.join("face.ttf")).unwrap();
    std::fs::remove_file(addon.join("main.lua")).unwrap();
    std::fs::remove_file(addon.join("FontFixture.toc")).unwrap();
    std::fs::remove_dir(addon).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn live_font_inheritance_reflows_spacing_and_preserves_style_overrides() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
        local root=CreateFont('RootFont');root:SetFont('fallback',18)
        local derived=CreateFont('DerivedFont');derived:SetFontObject(root)
        local t=UIParent:CreateFontString(nil,'OVERLAY','DerivedFont');t:SetPoint('TOPLEFT');t:SetText('AVATAR')
        local height=t:GetHeight();root:SetSpacing(3)
        assert(math.abs(t:GetHeight()-height-3)<0.00001,'live inherited spacing must invalidate layout')
        t:SetTextColor(0.2,0.4,0.6,1);root:SetTextColor(1,0,0,1)
        derived:SetFont('fallback',24)
        assert(select(2,t:GetFont())==24 and t:GetHeight()>height)
        t:SetFont('fallback',16);root:SetSpacing(5)
        assert(select(2,t:GetFont())==16 and t:GetSpacing()==5,'font override must retain inherited style')
        assert(not pcall(function() root:SetFontObject(derived) end),'font cycles must be refused')
    "#).unwrap();
    let snap = sim.snapshot().unwrap();
    assert_eq!(
        snap.regions.last().unwrap().visual.color,
        [0.2, 0.4, 0.6, 1.0]
    );
}

#[test]
fn native_templates_work_without_inventing_untyped_globals() {
    let sim = Sim::new().unwrap();
    sim.eval(r#"
        assert(rawget(_G,'GameFontHighlightSmall')==nil)
        local t=UIParent:CreateFontString(nil,'OVERLAY','GameFontHighlightSmall')
        t:SetText('ready');t:SetPoint('TOPLEFT')
        assert(t:GetText()=='ready' and t:GetStringWidth()>0 and select(2,t:GetFont())==10)
        assert(rawget(_G,'GameFontHighlightSmall')==nil)
        assert(not pcall(function() UIParent:CreateFontString(nil,'OVERLAY','NotARealFontTemplate') end))
    "#).unwrap();
}
