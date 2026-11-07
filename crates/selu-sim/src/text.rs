//! Deterministic, font-backed Latin text layout and snapshot glyph masks.
//! Positions are local pixels, top-left origin; y on a glyph is its baseline.
use crate::core::VisualStyle;
use ab_glyph::{Font, FontArc, GlyphId, ScaleFont};
use serde::{Deserialize, Serialize};

/// WoW accepts numbered arguments in localized format strings. Keep ordinary
/// Lua formatting on the original fast path and delegate conversion rules to
/// that formatter after reordering only the argument references.
pub(crate) fn install_format(lua: &mlua::Lua) -> mlua::Result<()> {
    let formatter: mlua::Function = crate::host_lua::factory(
        lua,
        "selu.client_string_format",
        r#"
        local native_format=string.format
        return function(pattern,...)
            if not string.find(pattern,'%%%d+%$') then return native_format(pattern,...) end
            local source={...}
            local output,arguments={},{}
            local cursor,implicit,count=1,1,0
            while cursor<=#pattern do
                local first=string.find(pattern,'%',cursor,true)
                if not first then table.insert(output,string.sub(pattern,cursor));break end
                table.insert(output,string.sub(pattern,cursor,first-1))
                if string.sub(pattern,first+1,first+1)=='%' then
                    table.insert(output,'%%');cursor=first+2
                else
                    local tail=string.sub(pattern,first+1)
                    local position,conversion=string.match(tail,'^(%d+)%$([-+ #0]*%d*%.?%d*[cdiouxXeEfgGqs])')
                    local consumed
                    if position then consumed=#position+1+#conversion;position=tonumber(position)
                    else
                        conversion=string.match(tail,'^([-+ #0]*%d*%.?%d*[cdiouxXeEfgGqs])')
                        if not conversion then return native_format(pattern,...) end
                        consumed=#conversion;position=implicit;implicit=implicit+1
                    end
                    count=count+1;arguments[count]=source[position]
                    table.insert(output,'%'..conversion);cursor=first+1+consumed
                end
            end
            return native_format(table.concat(output),unpack(arguments,1,count))
        end
    "#,
    )?;
    lua.globals()
        .raw_get::<mlua::Table>("string")?
        .raw_set("format", formatter.clone())?;
    lua.set_named_registry_value("selu.client_format", formatter.clone())?;
    lua.globals().raw_set("format", formatter)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Content {
    Glyph(u16),
    Texture {
        path: String,
        width: f32,
        height: f32,
        uv: [f32; 4],
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub content: Content,
    pub x: f32,
    pub y: f32,
    pub color: [f32; 4],
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphMask {
    pub id: u16,
    pub left: f32,
    pub top: f32,
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    pub start: usize,
    pub end: usize,
    pub x: f64,
    pub y: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextLayout {
    pub font_size: f64,
    pub width: f64,
    pub height: f64,
    pub line_height: f32,
    pub lines: usize,
    pub truncated: bool,
    pub placements: Vec<Placement>,
    pub masks: Vec<GlyphMask>,
    #[serde(default)]
    pub line_ranges: Vec<TextLine>,
}
#[derive(Clone)]
enum Atom {
    Char(char, [f32; 4]),
    Icon(String, f32, f32, f32, f32, [f32; 4]),
}
impl Atom {
    fn space(&self) -> bool {
        matches!(self, Self::Char(c, _) if c.is_whitespace())
    }
}
fn rgba(v: u32) -> [f32; 4] {
    [
        ((v >> 16) & 255) as f32 / 255.,
        ((v >> 8) & 255) as f32 / 255.,
        (v & 255) as f32 / 255.,
        (v >> 24) as f32 / 255.,
    ]
}
fn atoms(text: &str, style: &VisualStyle, resources: Option<&crate::fonts::FontBook>) -> Vec<Atom> {
    let mut result = Vec::new();
    let mut color = style.color;
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("|cn") {
            if let Some(end) = tail.find(':') {
                color = resources
                    .and_then(|r| r.color(&tail[..end]))
                    .map(rgba)
                    .unwrap_or(style.color);
                rest = &tail[end + 1..];
            } else {
                rest = "";
            }
            continue;
        }
        if let Some(tail) = rest.strip_prefix("|c")
            && tail.len() >= 8
            && tail.is_char_boundary(8)
            && let Ok(v) = u32::from_str_radix(&tail[..8], 16)
        {
            color = [
                ((v >> 16) & 255) as f32 / 255.0,
                ((v >> 8) & 255) as f32 / 255.0,
                (v & 255) as f32 / 255.0,
                (v >> 24) as f32 / 255.0,
            ];
            rest = &tail[8..];
            continue;
        }
        if let Some(tail) = rest.strip_prefix("|r") {
            color = style.color;
            rest = tail;
            continue;
        }
        if let Some(tail) = rest.strip_prefix("|H")
            && let Some(end) = tail.find("|h")
        {
            rest = &tail[end + 2..];
            continue;
        }
        if let Some(tail) = rest.strip_prefix("|h") {
            rest = tail;
            continue;
        }
        if let Some(tail) = rest.strip_prefix("|n") {
            result.push(Atom::Char('\n', color));
            rest = tail;
            continue;
        }
        if let Some(tail) = rest.strip_prefix("|A") {
            if let Some(end) = tail.find("|a") {
                let fields: Vec<_> = tail[..end].trim_start_matches(':').split(':').collect();
                if let Some(atlas) = resources.and_then(|r| r.assets.atlas(fields[0])) {
                    let number = |i: usize, default: f32| {
                        fields
                            .get(i)
                            .and_then(|s| s.parse::<f32>().ok())
                            .filter(|v| v.is_finite())
                            .unwrap_or(default)
                    };
                    let h = number(1, 0.);
                    let w = number(2, 0.);
                    result.push(Atom::Icon(
                        atlas.file_data_id.to_string(),
                        if w > 0. { w } else { atlas.width as f32 },
                        if h > 0. { h } else { atlas.height as f32 },
                        number(3, 0.),
                        number(4, 0.),
                        [
                            atlas.left as f32 / atlas.sheet_width as f32,
                            atlas.right as f32 / atlas.sheet_width as f32,
                            atlas.top as f32 / atlas.sheet_height as f32,
                            atlas.bottom as f32 / atlas.sheet_height as f32,
                        ],
                    ));
                }
                rest = &tail[end + 2..];
            } else {
                rest = "";
            }
            continue;
        }
        if let Some(tail) = rest.strip_prefix("|T")
            && let Some(end) = tail.find("|t")
        {
            let fields: Vec<_> = tail[..end].trim_start_matches(':').split(':').collect();
            let number = |i: usize, default: f32| {
                fields
                    .get(i)
                    .and_then(|s| s.parse::<f32>().ok())
                    .filter(|n| n.is_finite())
                    .unwrap_or(default)
            };
            let h = number(1, style.size as f32);
            let h = if h > 0.0 { h } else { style.size as f32 };
            let w = number(2, h);
            let w = if w > 0.0 { w } else { h };
            let (tw, th) = (number(5, w), number(6, h));
            let uv = if fields.len() >= 11 && tw > 0.0 && th > 0.0 {
                [
                    number(7, 0.0) / tw,
                    number(8, tw) / tw,
                    number(9, 0.0) / th,
                    number(10, th) / th,
                ]
            } else {
                [0.0, 1.0, 0.0, 1.0]
            };
            result.push(Atom::Icon(
                fields[0].into(),
                w,
                h,
                number(3, 0.0),
                number(4, 0.0),
                uv,
            ));
            rest = &tail[end + 2..];
            continue;
        }
        if let Some(tail) = rest.strip_prefix("||") {
            result.push(Atom::Char('|', color));
            rest = tail;
            continue;
        }
        if rest.starts_with("|H") || rest.starts_with("|T") {
            break;
        }
        if ["|c", "|a", "|t"].iter().any(|p| rest.starts_with(p)) {
            rest = &rest[2..];
            continue;
        }
        let c = rest.chars().next().unwrap();
        result.push(Atom::Char(c, color));
        rest = &rest[c.len_utf8()..];
    }
    result
}
pub fn pixel_scale(face: &FontArc, size: f64) -> f32 {
    let em = face.units_per_em().unwrap_or(face.height_unscaled());
    size as f32 * face.height_unscaled() / em
}
struct Shaper<'a> {
    face: harfrust::FontRef<'a>,
    data: harfrust::ShaperData,
    scale: f32,
}
impl<'a> Shaper<'a> {
    fn new(face: &'a FontArc, size: f64) -> Self {
        let scale = size as f32 / face.units_per_em().unwrap_or(face.height_unscaled());
        let face = harfrust::FontRef::new(face.font_data()).expect("validated OpenType face");
        let data = harfrust::ShaperData::new(&face);
        Self { face, data, scale }
    }
    fn row(&self, row: &[Atom], emit: bool) -> (f32, Vec<Placement>) {
        let mut x = 0.0;
        let mut out = Vec::new();
        let mut i = 0;
        while i < row.len() {
            match &row[i] {
                Atom::Icon(path, w, h, ox, oy, uv) => {
                    if emit {
                        out.push(Placement {
                            content: Content::Texture {
                                path: path.clone(),
                                width: *w,
                                height: *h,
                                uv: *uv,
                            },
                            x: x + ox,
                            y: -oy,
                            color: [1.0; 4],
                        });
                    }
                    x += w;
                    i += 1;
                }
                Atom::Char(_, _) => {
                    let mut text = String::new();
                    let mut colors = Vec::new();
                    while i < row.len() {
                        let Atom::Char(c, color) = &row[i] else { break };
                        colors.push((text.len(), *color));
                        text.push(*c);
                        i += 1;
                    }
                    let mut buffer = harfrust::UnicodeBuffer::new();
                    buffer.push_str(&text);
                    buffer.guess_segment_properties();
                    let shaped = self
                        .data
                        .shaper(&self.face)
                        .build()
                        .shape(buffer, Default::default());
                    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                        if emit {
                            let cluster = info.cluster as usize;
                            let index = colors
                                .partition_point(|(byte, _)| *byte <= cluster)
                                .saturating_sub(1);
                            out.push(Placement {
                                content: Content::Glyph(info.glyph_id as u16),
                                x: x + pos.x_offset as f32 * self.scale,
                                y: -(pos.y_offset as f32) * self.scale,
                                color: colors[index].1,
                            });
                        }
                        x += pos.x_advance as f32 * self.scale;
                    }
                }
            }
        }
        (x, out)
    }
    fn width(&self, row: &[Atom]) -> f32 {
        self.row(row, false).0
    }
}
fn trim(row: &mut Vec<Atom>) {
    while row.last().is_some_and(Atom::space) {
        row.pop();
    }
}

pub fn layout(
    face: &FontArc,
    text: &str,
    style: &VisualStyle,
    box_width: Option<f64>,
    box_height: Option<f64>,
) -> TextLayout {
    layout_with_resources(face, text, style, box_width, box_height, None)
}
pub(crate) fn layout_with_resources(
    face: &FontArc,
    text: &str,
    style: &VisualStyle,
    box_width: Option<f64>,
    box_height: Option<f64>,
    resources: Option<&crate::fonts::FontBook>,
) -> TextLayout {
    let font = face.clone().into_scaled(pixel_scale(face, style.size));
    let line_height =
        (font.ascent() - font.descent() + font.line_gap() + style.spacing as f32).max(1.0);
    let limit = box_width
        .filter(|w| w.is_finite() && *w > 0.0)
        .map(|w| w as f32)
        .unwrap_or(f32::INFINITY);
    let shaper = Shaper::new(face, style.size);
    let input = atoms(text, style, resources);
    let mut rows: Vec<Vec<Atom>> = Vec::new();
    let mut starts = Vec::new();
    let mut row_start = 0;
    let mut row = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if matches!(input[i], Atom::Char('\n', _)) {
            starts.push(row_start);
            rows.push(std::mem::take(&mut row));
            row_start = i + 1;
            i += 1;
            continue;
        }
        if input[i].space() {
            row.push(input[i].clone());
            i += 1;
            continue;
        }
        let end = input[i..]
            .iter()
            .position(Atom::space)
            .map(|j| i + j)
            .unwrap_or(input.len());
        let word = &input[i..end];
        let mut candidate = row.clone();
        candidate.extend_from_slice(word);
        if !style.wrap || !limit.is_finite() || shaper.width(&candidate) <= limit {
            row = candidate;
            i = end;
            continue;
        }
        if !row.is_empty() {
            trim(&mut row);
            if !row.is_empty() {
                starts.push(row_start);
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        }
        row_start = i;
        if !style.non_space_wrap {
            row.extend_from_slice(word);
            i = end;
            continue;
        }
        // Shape whole prefixes, preserving ligatures and kerning at each new
        // line boundary. Binary search avoids rescanning every growing prefix.
        let mut start = 0;
        while start < word.len() {
            let (mut low, mut high) = (0, word.len() - start);
            while low < high {
                let mid = (low + high).div_ceil(2);
                if shaper.width(&word[start..start + mid]) <= limit {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            let count = low.max(1);
            row.extend_from_slice(&word[start..start + count]);
            start += count;
            if start < word.len() {
                starts.push(row_start);
                rows.push(std::mem::take(&mut row));
                row_start = i + start;
            }
        }
        i = end;
    }
    starts.push(row_start);
    rows.push(row);
    let row_height = |row: &[Atom]| {
        row.iter().fold(line_height, |h, a| match a {
            Atom::Icon(_, _, ih, _, _, _) => h.max(*ih),
            _ => h,
        })
    };
    let mut max_rows = if style.max_lines == 0 {
        usize::MAX
    } else {
        style.max_lines
    };
    if let Some(h) = box_height.filter(|h| h.is_finite() && *h >= 0.0) {
        let mut used = 0.0;
        let mut count = 0;
        for row in &rows {
            // Font metrics determine baseline advance, not whether the last
            // line fits. WoW admits a nominal font-height line even when the
            // face's ascent/descent and line gap exceed its em size. Inline
            // textures still require their actual height.
            let required = row
                .iter()
                .fold(style.size as f32, |height, atom| match atom {
                    Atom::Icon(_, _, ih, _, _, _) => height.max(*ih),
                    _ => height,
                });
            if used + required > h as f32 + 0.001 {
                break;
            }
            used += row_height(row);
            count += 1;
        }
        max_rows = max_rows.min(count);
    }
    let hidden_rows = rows.len() > max_rows;
    let mut truncated = hidden_rows;
    rows.truncate(max_rows);
    let row_count = rows.len();
    for (i, row) in rows.iter_mut().enumerate() {
        let overflow = shaper.width(row) > limit;
        if overflow || (hidden_rows && i + 1 == row_count) {
            truncated = true;
            let color = row
                .iter()
                .rev()
                .find_map(|a| {
                    if let Atom::Char(_, c) = a {
                        Some(*c)
                    } else {
                        None
                    }
                })
                .unwrap_or(style.color);
            let ellipsis = if style.ellipsis {
                vec![Atom::Char('…', color)]
            } else {
                Vec::new()
            };
            loop {
                let mut candidate = row.clone();
                candidate.extend_from_slice(&ellipsis);
                if row.is_empty() || shaper.width(&candidate) <= limit {
                    break;
                }
                row.pop();
            }
            if shaper.width(&ellipsis) <= limit {
                row.extend(ellipsis);
            }
        }
    }
    let total_height: f32 = rows.iter().map(|r| row_height(r)).sum();
    let dy = match style.justify_v.as_str() {
        "BOTTOM" => box_height.unwrap_or(total_height as f64) as f32 - total_height,
        "MIDDLE" => (box_height.unwrap_or(total_height as f64) as f32 - total_height) * 0.5,
        _ => 0.0,
    };
    let mut out = TextLayout {
        font_size: style.size,
        height: total_height as f64,
        line_height,
        lines: rows.len(),
        truncated,
        ..Default::default()
    };
    let mut y = dy;
    for (row, start) in rows.into_iter().zip(starts) {
        let w = shaper.width(&row);
        out.width = out.width.max(w as f64);
        let dx = match style.justify_h.as_str() {
            "RIGHT" => box_width.unwrap_or(w as f64) as f32 - w,
            "CENTER" => (box_width.unwrap_or(w as f64) as f32 - w) * 0.5,
            _ => 0.0,
        };
        let h = row_height(&row);
        out.line_ranges.push(TextLine {
            start,
            end: start + row.len(),
            x: dx as f64,
            y: y as f64,
            height: h as f64,
        });
        let (_, placements) = shaper.row(&row, true);
        for mut p in placements {
            p.x += dx;
            p.y += y + match p.content {
                Content::Glyph(_) => font.ascent(),
                Content::Texture { height, .. } => (h - height) * 0.5,
            };
            out.placements.push(p);
        }
        y += h;
    }
    out
}

/// Rasterize each distinct glyph once. Studio consumes these exact masks and
/// baseline positions, rather than invoking a second text shaping engine.
pub fn rasterize(face: &FontArc, size: f64, layout: &mut TextLayout) {
    let mut ids: Vec<_> = layout
        .placements
        .iter()
        .filter_map(|p| {
            if let Content::Glyph(id) = p.content {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let glyph = GlyphId(id).with_scale(pixel_scale(face, size));
        if let Some(outline) = face.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            let (w, h) = (bounds.width() as usize, bounds.height() as usize);
            if w == 0 || h == 0 || w.saturating_mul(h) > 1024 * 1024 {
                continue;
            }
            let mut alpha = vec![0; w * h];
            outline.draw(|x, y, v| alpha[y as usize * w + x as usize] = (v * 255.0).round() as u8);
            layout.masks.push(GlyphMask {
                id,
                left: bounds.min.x,
                top: bounds.min.y,
                width: w,
                height: h,
                alpha,
            });
        }
    }
}

#[cfg(test)]
pub(crate) fn expected_fallback(text: &str, size: f32) -> (f64, f64) {
    // Independent reference from the bundled file's unscaled font tables.
    let face = ab_glyph::FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).unwrap();
    let units = face.units_per_em().unwrap();
    let mut width = 0.0;
    let mut previous = None;
    for c in text.chars() {
        let id = face.glyph_id(c);
        width += face.h_advance_unscaled(id);
        if let Some(p) = previous {
            width += face.kern_unscaled(p, id);
        }
        previous = Some(id);
    }
    (
        (width * size / units) as f64,
        ((face.ascent_unscaled() - face.descent_unscaled() + face.line_gap_unscaled()) * size
            / units) as f64,
    )
}

#[cfg(test)]
mod markup_tests {
    use super::*;
    #[test]
    fn localized_format_reorders_arguments_without_changing_lua_conversions() {
        let s = crate::Session::new(Default::default()).unwrap();
        install_format(s.lua()).unwrap();
        s.eval("assert(string.format('%2$s/%1$03d/%2$s',7,'two')=='two/007/two');assert(format('%3$s','one',nil,'three')=='three');assert(format('%%1$s %1$s','literal')=='%1$s literal');assert(format('%04d %.2f',7,1.25)=='0007 1.25');assert(not pcall(format,'%1$d','invalid'));table={};string={};assert(format('%2$s/%1$d',7,'two')=='two/7')").unwrap();
    }
    #[test]
    fn formatted_widget_text_uses_the_captured_client_formatter() {
        let s = crate::Session::new(Default::default()).unwrap();
        install_format(s.lua()).unwrap();
        s.eval("local label=UIParent:CreateFontString();label:SetFormattedText('%2$s %1$03d',7,'sample');assert(label:GetText()=='sample 007');table={};string={};label:SetFormattedText('%2$s %1$d',8,'sample');assert(label:GetText()=='sample 8')").unwrap();
    }
    fn plain(a: &[Atom]) -> String {
        a.iter()
            .filter_map(|a| {
                if let Atom::Char(c, _) = a {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn markup_controls_never_become_visible_glyphs() {
        let mut book = crate::fonts::FontBook::default();
        book.set_colors(&[("SELF_WRITTEN_COLOR".into(), 0xff336699)]);
        let style = VisualStyle::default();
        let a = atoms(
            "|cnSELF_WRITTEN_COLOR:abc|r|cffff0000R|r|Hitem:123|h[link]|h|n|Tself-art:8:9|t|A:self-missing:8:9|a||",
            &style,
            Some(&book),
        );
        assert_eq!(plain(&a), "abcR[link]\n|");
        assert!(matches!(a[0],Atom::Char('a',c) if c==rgba(0xff336699)));
        assert!(matches!(a[3],Atom::Char('R',c) if c==[1.,0.,0.,1.]));
        assert!(
            matches!(a.iter().find(|a|matches!(a,Atom::Icon(..))),Some(Atom::Icon(p,w,h,..)) if p=="self-art" && *w==9. && *h==8.)
        );
        assert_eq!(plain(&atoms("|cnUNKNOWN_COLOR:X|rY", &style, None)), "XY");
        assert!(
            matches!(atoms("|cnUNKNOWN_COLOR:X",&style,None)[0],Atom::Char('X',c) if c==style.color)
        );
        for s in ["|cnmalformed", "|Hmissing", "|Tmissing", "|Amissing"] {
            assert!(atoms(s, &style, None).is_empty());
        }
        assert_eq!(plain(&atoms("||cnliteral", &style, None)), "|cnliteral");
    }
    #[test]
    #[ignore = "requires SELU_CLIENT_INSTALL"]
    fn client_named_colors_and_atlas_markup_use_runtime_data() {
        let s = crate::Session::new(crate::SessionConfig {
            target: Some("wowf".into()),
            client_install: Some(std::env::var_os("SELU_CLIENT_INSTALL").unwrap().into()),
            ..Default::default()
        })
        .unwrap();
        s.eval("C_UIColor={}").unwrap();
        crate::client_ui::install(s.lua(), s.sim().ctx()).unwrap();
        let store = crate::frames::arena::store(s.lua());
        let a = store.borrow();
        let storage = a.fonts.assets.client_storage().unwrap();
        let bytes = storage.read(4328468).unwrap();
        let table = selu_gamedata::db2::Table::parse(&bytes).unwrap();
        let key = table.text(0, 0).unwrap();
        let expected = rgba(table.value(0, 1).unwrap());
        let layout = a.fonts.layout(
            &format!("|cn{key}:X|r"),
            &VisualStyle::default(),
            None,
            None,
        );
        assert_eq!(layout.placements[0].color, expected);
        let atlas = a.fonts.assets.atlas("UI-Character-Info-Title").unwrap();
        let layout = a.fonts.layout(
            "|A:UI-Character-Info-Title:10:20|a",
            &VisualStyle::default(),
            None,
            None,
        );
        assert!(
            matches!(&layout.placements[0].content,Content::Texture{path,width,height,..} if path==&atlas.file_data_id.to_string() && *width==20. && *height==10.)
        );
    }
}
