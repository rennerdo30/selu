//! The editor's user interface: Explorer tree, tab strip, breadcrumbs,
//! find bar, quick open, the code view with a painted gutter and current
//! line, completion popup, hover, and the Problems list.

use super::check::{Problem, Severity};
use super::complete::{self, Context};
use super::highlight::{self, Kind};
use super::{Completion, Editor, byte_to_char, char_to_byte, color, problem_range};
use crate::theme::{caption, icon, icon_font, palette};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Modifiers, Sense, Shape, Stroke,
    TextFormat, Vec2, text::LayoutJob,
};
use std::collections::BTreeMap;

pub struct Find {
    pub query: String,
    pub case: bool,
    /// Index into the matches of the current query.
    pub current: usize,
    pub focus: bool,
}

pub struct QuickOpen {
    pub query: String,
    pub selected: usize,
}

fn file_icon(name: &str) -> (&'static str, Color32) {
    if name.ends_with(".selu") {
        (icon::CODE, palette::ACCENT)
    } else if name.ends_with(".toml") {
        (icon::SETTINGS, palette::MUTED)
    } else {
        (icon::DOCUMENT, palette::MUTED)
    }
}

fn find_matches(text: &str, find: &Find) -> Vec<std::ops::Range<usize>> {
    if find.query.is_empty() {
        return Vec::new();
    }
    let (hay, needle) = if find.case {
        (text.to_string(), find.query.clone())
    } else {
        (text.to_lowercase(), find.query.to_lowercase())
    };
    // Lowercasing can change byte lengths for some scripts; only use the
    // matches when the lengths agree (always true for ASCII).
    if hay.len() != text.len() {
        return Vec::new();
    }
    hay.match_indices(&needle)
        .map(|(i, m)| i..i + m.len())
        .collect()
}

impl Editor {
    pub fn project_name(&self) -> String {
        self.project
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "No project".into(), |n| n.to_string_lossy().into_owned())
    }

    pub fn counts(&self) -> (usize, usize) {
        let errors = self
            .problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .count();
        (errors, self.problems.len() - errors)
    }

    pub fn cursor_line_col(&self) -> Option<(usize, usize)> {
        self.cursor_pos
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn target_label(&self) -> Option<String> {
        self.api.as_ref().map(|a| a.target.clone())
    }

    pub fn active_rel(&self) -> Option<&str> {
        self.open.get(self.active).map(|b| b.rel.as_str())
    }

    /// Shortcuts that work anywhere in the window.
    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        let (save, quick, find, close, zoom_in, zoom_out, zoom_reset) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::S),
                i.consume_key(Modifiers::COMMAND, Key::P),
                i.consume_key(Modifiers::COMMAND, Key::F),
                i.consume_key(Modifiers::COMMAND, Key::W),
                i.consume_key(Modifiers::COMMAND, Key::Plus)
                    || i.consume_key(Modifiers::COMMAND, Key::Equals),
                i.consume_key(Modifiers::COMMAND, Key::Minus),
                i.consume_key(Modifiers::COMMAND, Key::Num0),
            )
        });
        if save {
            self.save_all();
        }
        if quick && self.project.is_some() {
            self.quick_open = Some(QuickOpen {
                query: String::new(),
                selected: 0,
            });
        }
        if find && !self.open.is_empty() {
            let query = self
                .find
                .as_ref()
                .map(|f| f.query.clone())
                .unwrap_or_default();
            self.find = Some(Find {
                query,
                case: false,
                current: 0,
                focus: true,
            });
        }
        if close && !self.open.is_empty() {
            self.close_tab(self.active);
        }
        if zoom_in {
            self.font_size = (self.font_size + 1.0).min(28.0);
        }
        if zoom_out {
            self.font_size = (self.font_size - 1.0).max(9.0);
        }
        if zoom_reset {
            self.font_size = 14.0;
        }
    }

    pub fn open_quick_open(&mut self) {
        if self.project.is_some() {
            self.quick_open = Some(QuickOpen {
                query: String::new(),
                selected: 0,
            });
        }
    }

    pub fn open_find(&mut self) {
        if !self.open.is_empty() {
            self.find = Some(Find {
                query: String::new(),
                case: false,
                current: 0,
                focus: true,
            });
        }
    }

    pub fn zoom(&mut self, delta: f32) {
        self.font_size = (self.font_size + delta).clamp(9.0, 28.0);
    }

    pub fn close_tab(&mut self, i: usize) {
        if i >= self.open.len() {
            return;
        }
        if self.open[i].dirty() {
            self.save(i);
        }
        self.open.remove(i);
        if self.active >= i && self.active > 0 {
            self.active -= 1;
        }
        self.active = self.active.min(self.open.len().saturating_sub(1));
    }

    // -----------------------------------------------------------------
    // Explorer
    // -----------------------------------------------------------------

    pub fn ui_explorer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            caption(ui, "Explorer");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if self.project.is_some()
                    && ui
                        .add(
                            egui::Button::new(crate::theme::icon_text(icon::ADD, 12.0))
                                .frame(false),
                        )
                        .on_hover_text("New file in src/")
                        .clicked()
                {
                    self.new_file = Some(String::new());
                }
            });
        });
        if self.project.is_none() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("No project open").color(palette::MUTED));
            ui.label(
                egui::RichText::new(
                    "Open a Selu project with scripts\\studio.bat <folder> or `selu debug`.",
                )
                .small()
                .color(palette::FAINT),
            );
            return;
        }
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(self.project_name().to_uppercase())
                .strong()
                .size(12.0)
                .color(palette::TEXT),
        );
        ui.add_space(2.0);

        if let Some(name) = &mut self.new_file {
            let mut done = None;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("src/").color(palette::MUTED));
                let r = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text("name.selu")
                        .desired_width(140.0),
                );
                r.request_focus();
                if r.lost_focus() {
                    done = Some(ui.input(|i| i.key_pressed(Key::Enter)));
                }
            });
            match done {
                Some(true) => {
                    let mut n = self.new_file.take().unwrap_or_default().trim().to_string();
                    if !n.is_empty() {
                        if !n.ends_with(".selu") {
                            n.push_str(".selu");
                        }
                        self.create_file(&format!("src/{n}"));
                    }
                }
                Some(false) => self.new_file = None,
                None => {}
            }
        }

        // Folder tree from the flat file list.
        let mut tree: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in &self.files {
            let dir = f.rsplit_once('/').map_or("", |(d, _)| d).to_string();
            tree.entry(dir).or_default().push(f.clone());
        }
        let mut open_file = None;
        let mut toggle = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let dirs: Vec<String> = tree.keys().cloned().collect();
                for dir in dirs {
                    let depth = if dir.is_empty() {
                        0
                    } else {
                        dir.matches('/').count() + 1
                    };
                    if !dir.is_empty() {
                        let collapsed = self.collapsed.contains(&dir);
                        // Hide folders inside a collapsed folder.
                        if self
                            .collapsed
                            .iter()
                            .any(|c| dir.starts_with(&format!("{c}/")))
                        {
                            continue;
                        }
                        let name = dir.rsplit('/').next().unwrap_or(&dir);
                        let row = self.tree_row(
                            ui,
                            depth - 1,
                            if collapsed {
                                icon::CHEVRON_RIGHT
                            } else {
                                icon::CHEVRON_DOWN
                            },
                            palette::MUTED,
                            name,
                            false,
                            false,
                            0,
                        );
                        if row.clicked() {
                            toggle = Some(dir.clone());
                        }
                        if collapsed {
                            continue;
                        }
                    }
                    if self
                        .collapsed
                        .iter()
                        .any(|c| dir.starts_with(&format!("{c}/")) || &dir == c)
                    {
                        continue;
                    }
                    for f in &tree[&dir] {
                        let name = f.rsplit('/').next().unwrap_or(f);
                        let (icon, icon_color) = file_icon(name);
                        let active = self.open.get(self.active).is_some_and(|b| &b.rel == f);
                        let dirty = self.open.iter().any(|b| &b.rel == f && b.dirty());
                        let errors = self
                            .problems
                            .iter()
                            .filter(|p| {
                                p.file.as_deref() == Some(f.as_str())
                                    && p.severity == Severity::Error
                            })
                            .count();
                        let row =
                            self.tree_row(ui, depth, icon, icon_color, name, active, dirty, errors);
                        if row.clicked() {
                            open_file = Some(f.clone());
                        }
                    }
                }
            });
        if let Some(d) = toggle
            && !self.collapsed.remove(&d)
        {
            self.collapsed.insert(d);
        }
        if let Some(f) = open_file {
            self.open_file(&f);
        }
    }

    #[allow(clippy::too_many_arguments)] // one row of the tree; a struct adds nothing
    fn tree_row(
        &self,
        ui: &mut egui::Ui,
        depth: usize,
        icon: &str,
        icon_color: Color32,
        name: &str,
        active: bool,
        dirty: bool,
        errors: usize,
    ) -> egui::Response {
        let height = 24.0;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
        let painter = ui.painter();
        if active {
            painter.rect_filled(rect, CornerRadius::same(4), palette::ACCENT_SOFT);
        } else if response.hovered() {
            painter.rect_filled(rect, CornerRadius::same(4), palette::HOVER);
        }
        let x = rect.left() + 8.0 + depth as f32 * 14.0;
        painter.text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            icon_font(12.0),
            icon_color,
        );
        let name_color = if errors > 0 {
            palette::ERROR
        } else if active {
            Color32::WHITE
        } else {
            palette::TEXT
        };
        painter.text(
            egui::pos2(x + 18.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.0),
            name_color,
        );
        let mut right = rect.right() - 10.0;
        if errors > 0 {
            let badge = format!("{errors}");
            let galley = painter.layout_no_wrap(badge, FontId::proportional(11.0), Color32::WHITE);
            let w = galley.size().x + 10.0;
            let r = egui::Rect::from_center_size(
                egui::pos2(right - w / 2.0, rect.center().y),
                egui::vec2(w, 16.0),
            );
            painter.rect_filled(
                r,
                CornerRadius::same(8),
                palette::ERROR.gamma_multiply(0.85),
            );
            painter.galley(r.center() - galley.size() / 2.0, galley, Color32::WHITE);
            right -= w + 6.0;
        }
        if dirty {
            painter.circle_filled(egui::pos2(right - 4.0, rect.center().y), 4.0, palette::TEXT);
        }
        response
    }

    // -----------------------------------------------------------------
    // Problems
    // -----------------------------------------------------------------

    pub fn ui_problems(&mut self, ui: &mut egui::Ui) {
        if self.problems.is_empty() {
            ui.add_space(6.0);
            let (text, c) = if self.check_ok.is_some() {
                (
                    "No problems have been detected in the project.",
                    palette::MUTED,
                )
            } else {
                ("Checking the project...", palette::FAINT)
            };
            ui.label(egui::RichText::new(text).color(c));
            return;
        }
        let mut go = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, p) in self.problems.iter().enumerate() {
                    let (icon, col) = match p.severity {
                        Severity::Error => (icon::ERROR, palette::ERROR),
                        Severity::Warning => (icon::WARNING, palette::WARNING),
                    };
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 24.0),
                        Sense::click(),
                    );
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(4), palette::HOVER);
                    }
                    let painter = ui.painter();
                    let y = rect.center().y;
                    painter.text(
                        egui::pos2(rect.left() + 8.0, y),
                        Align2::LEFT_CENTER,
                        icon,
                        icon_font(13.0),
                        col,
                    );
                    let msg = painter.text(
                        egui::pos2(rect.left() + 28.0, y),
                        Align2::LEFT_CENTER,
                        &p.message,
                        FontId::proportional(13.0),
                        palette::TEXT,
                    );
                    let place = match &p.file {
                        Some(f) => format!(
                            "{}  [{}, {}]",
                            f.strip_prefix("src/").unwrap_or(f),
                            p.line,
                            p.column
                        ),
                        None => String::new(),
                    };
                    painter.text(
                        egui::pos2(msg.right() + 10.0, y),
                        Align2::LEFT_CENTER,
                        format!("{}  {place}", p.code),
                        FontId::proportional(12.0),
                        palette::FAINT,
                    );
                    let response = if p.notes.is_empty() {
                        response
                    } else {
                        response.on_hover_text(p.notes.join("\n"))
                    };
                    if response.clicked() {
                        go = Some(i);
                    }
                }
            });
        if let Some(i) = go {
            self.go_to(i);
        }
    }

    // -----------------------------------------------------------------
    // Quick open (Ctrl+P)
    // -----------------------------------------------------------------

    pub fn ui_quick_open(&mut self, ctx: &egui::Context) {
        let Some(q) = &mut self.quick_open else {
            return;
        };
        let needle = q.query.to_lowercase();
        let hits: Vec<String> = self
            .files
            .iter()
            .filter(|f| needle.is_empty() || f.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        q.selected = q.selected.min(hits.len().saturating_sub(1));
        let (mut open, mut close) = (None, false);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("quick-open"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.center().x - 280.0, screen.top() + 70.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(560.0);
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut q.query)
                                .hint_text("Search files by name")
                                .desired_width(f32::INFINITY),
                        );
                        r.request_focus();
                        ui.input_mut(|i| {
                            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                                q.selected = (q.selected + 1).min(hits.len().saturating_sub(1));
                            }
                            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                                q.selected = q.selected.saturating_sub(1);
                            }
                            if i.consume_key(Modifiers::NONE, Key::Enter) {
                                open = hits.get(q.selected).cloned();
                            }
                            if i.consume_key(Modifiers::NONE, Key::Escape) {
                                close = true;
                            }
                        });
                        ui.add_space(4.0);
                        for (i, f) in hits.iter().enumerate().take(14) {
                            let name = f.rsplit('/').next().unwrap_or(f);
                            let (icon, ic) = file_icon(name);
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 26.0),
                                Sense::click(),
                            );
                            if i == q.selected {
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    palette::ACCENT_SOFT,
                                );
                            } else if resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    palette::HOVER,
                                );
                            }
                            let p = ui.painter();
                            p.text(
                                rect.left_center() + Vec2::new(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                icon,
                                icon_font(12.0),
                                ic,
                            );
                            let n = p.text(
                                rect.left_center() + Vec2::new(26.0, 0.0),
                                Align2::LEFT_CENTER,
                                name,
                                FontId::proportional(13.5),
                                palette::TEXT,
                            );
                            p.text(
                                n.right_center() + Vec2::new(10.0, 0.0),
                                Align2::LEFT_CENTER,
                                f,
                                FontId::proportional(12.0),
                                palette::FAINT,
                            );
                            if resp.clicked() {
                                open = Some(f.clone());
                            }
                        }
                        if hits.is_empty() {
                            ui.label(
                                egui::RichText::new("No matching files").color(palette::FAINT),
                            );
                        }
                    });
            });
        if let Some(f) = open {
            self.quick_open = None;
            self.open_file(&f);
        } else if close {
            self.quick_open = None;
        }
    }

    // -----------------------------------------------------------------
    // The editor area
    // -----------------------------------------------------------------

    fn ui_tabs(&mut self, ui: &mut egui::Ui) {
        let mut activate = None;
        let mut close = None;
        egui::ScrollArea::horizontal()
            .id_salt("tabs")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, b) in self.open.iter().enumerate() {
                        let name = b.rel.rsplit('/').next().unwrap_or(&b.rel);
                        let (icon, ic) = file_icon(name);
                        let font = FontId::proportional(13.0);
                        let text_w = ui
                            .painter()
                            .layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE)
                            .size()
                            .x;
                        let w = text_w + 64.0;
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(w, 36.0), Sense::click());
                        let active = i == self.active;
                        let p = ui.painter();
                        let fill = if active {
                            palette::BG
                        } else if resp.hovered() {
                            palette::HOVER
                        } else {
                            palette::PANEL
                        };
                        p.rect_filled(rect, 0.0, fill);
                        if active {
                            p.rect_filled(
                                egui::Rect::from_min_size(
                                    rect.left_top(),
                                    egui::vec2(rect.width(), 2.0),
                                ),
                                0.0,
                                palette::ACCENT,
                            );
                        }
                        p.line_segment(
                            [rect.right_top(), rect.right_bottom()],
                            Stroke::new(1.0, palette::BORDER),
                        );
                        p.text(
                            rect.left_center() + Vec2::new(12.0, 0.0),
                            Align2::LEFT_CENTER,
                            icon,
                            icon_font(11.0),
                            ic,
                        );
                        p.text(
                            rect.left_center() + Vec2::new(28.0, 0.0),
                            Align2::LEFT_CENTER,
                            name,
                            font,
                            if active {
                                Color32::WHITE
                            } else {
                                palette::MUTED
                            },
                        );
                        let x_center = egui::pos2(rect.right() - 16.0, rect.center().y);
                        let x_rect = egui::Rect::from_center_size(x_center, egui::vec2(18.0, 18.0));
                        let over_x = resp.hover_pos().is_some_and(|pos| x_rect.contains(pos));
                        if b.dirty() && !over_x {
                            p.circle_filled(x_center, 4.0, palette::TEXT);
                        } else if resp.hovered() || active {
                            if over_x {
                                p.rect_filled(x_rect, CornerRadius::same(4), palette::RAISED);
                            }
                            p.text(
                                x_center,
                                Align2::CENTER_CENTER,
                                icon::CLOSE,
                                icon_font(10.0),
                                palette::MUTED,
                            );
                        }
                        let resp = resp.on_hover_text(&b.rel);
                        if resp.clicked() {
                            if over_x {
                                close = Some(i);
                            } else {
                                activate = Some(i);
                            }
                        }
                        if resp.middle_clicked() {
                            close = Some(i);
                        }
                    }
                });
            });
        if let Some(i) = activate {
            self.active = i;
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
    }

    fn ui_breadcrumbs(&self, ui: &mut egui::Ui, rel: &str) {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            let parts: Vec<&str> = rel.split('/').collect();
            for (i, part) in parts.iter().enumerate() {
                let last = i + 1 == parts.len();
                ui.label(egui::RichText::new(*part).size(12.0).color(if last {
                    palette::TEXT
                } else {
                    palette::MUTED
                }));
                if !last {
                    ui.label(egui::RichText::new("›").size(12.0).color(palette::FAINT));
                }
            }
        });
    }

    fn ui_find(&mut self, ui: &mut egui::Ui, matches: usize) -> (bool, bool) {
        let (mut next, mut prev, mut close) = (false, false, false);
        let Some(f) = &mut self.find else {
            return (false, false);
        };
        egui::Frame::new()
            .fill(palette::RAISED)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut f.query)
                            .hint_text("Find")
                            .desired_width(220.0),
                    );
                    if f.focus {
                        r.request_focus();
                        f.focus = false;
                    }
                    if r.changed() {
                        f.current = 0;
                    }
                    if r.has_focus() || r.lost_focus() {
                        ui.input_mut(|i| {
                            if i.consume_key(Modifiers::SHIFT, Key::Enter) {
                                prev = true;
                            } else if i.consume_key(Modifiers::NONE, Key::Enter) {
                                next = true;
                            }
                            if i.consume_key(Modifiers::NONE, Key::Escape) {
                                close = true;
                            }
                        });
                        if (next || prev) && !r.has_focus() {
                            r.request_focus();
                        }
                    }
                    let count = if f.query.is_empty() {
                        String::new()
                    } else if matches == 0 {
                        "No results".into()
                    } else {
                        format!("{} of {matches}", f.current.min(matches - 1) + 1)
                    };
                    ui.label(egui::RichText::new(count).size(12.0).color(palette::MUTED));
                    if ui
                        .add(egui::Button::selectable(f.case, "Aa"))
                        .on_hover_text("Match case")
                        .clicked()
                    {
                        f.case = !f.case;
                    }
                    if ui
                        .button(crate::theme::icon_text(icon::UP, 11.0))
                        .on_hover_text("Previous (Shift+Enter)")
                        .clicked()
                    {
                        prev = true;
                    }
                    if ui
                        .button(crate::theme::icon_text(icon::DOWN, 11.0))
                        .on_hover_text("Next (Enter)")
                        .clicked()
                    {
                        next = true;
                    }
                    if ui
                        .button(crate::theme::icon_text(icon::CLOSE, 10.0))
                        .on_hover_text("Close (Esc)")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if let Some(f) = &mut self.find
            && matches > 0
        {
            if next {
                f.current = (f.current + 1) % matches;
            }
            if prev {
                f.current = (f.current + matches - 1) % matches;
            }
        }
        if close {
            self.find = None;
        }
        (next, prev)
    }

    fn ui_welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.25);
            ui.label(
                egui::RichText::new("Selu Studio")
                    .size(30.0)
                    .color(palette::TEXT),
            );
            ui.label(
                egui::RichText::new("Write, run and debug World of Warcraft addons")
                    .color(palette::MUTED),
            );
            ui.add_space(24.0);
            for (keys, what) in [
                ("Ctrl+P", "Open a file"),
                ("Ctrl+S", "Save, rebuild and /reload"),
                ("Ctrl+F", "Find in the file"),
                ("Ctrl+Space", "Complete a name"),
                ("F5", "Run or pause the game clock"),
                ("Ctrl+R", "/reload the addon"),
            ] {
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() / 2.0 - 150.0);
                    ui.add_sized(
                        [150.0, 22.0],
                        egui::Label::new(egui::RichText::new(what).color(palette::MUTED)),
                    );
                    ui.label(egui::RichText::new(keys).monospace().color(palette::TEXT));
                });
            }
        });
    }

    pub fn ui_editor(&mut self, ui: &mut egui::Ui) {
        if self.open.is_empty() {
            self.ui_welcome(ui);
            return;
        }
        egui::Frame::new().fill(palette::PANEL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            self.ui_tabs(ui);
        });
        if self.open.is_empty() {
            self.ui_welcome(ui);
            return;
        }
        let active = self.active;
        let rel = self.open[active].rel.clone();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.ui_breadcrumbs(ui, &rel);
        });
        let matches: Vec<std::ops::Range<usize>> = match &self.find {
            Some(f) => find_matches(&self.open[active].text, f),
            None => Vec::new(),
        };
        let mut find_jump = false;
        if self.find.is_some() {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() - 470.0_f32.min(ui.available_width()));
                let (n, p) = self.ui_find(ui, matches.len());
                find_jump = n || p;
            });
        }
        let current_match = self.find.as_ref().and_then(|f| {
            matches
                .get(f.current.min(matches.len().saturating_sub(1)))
                .cloned()
        });
        if find_jump && let Some(m) = &current_match {
            self.jump_select = Some(m.clone());
        }
        ui.add_space(2.0);

        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        if self.locals_stale {
            let texts: Vec<&str> = self.open.iter().map(|b| b.text.as_str()).collect();
            self.locals = complete::local_words(&texts);
            self.locals_stale = false;
        }
        let id = egui::Id::unique(("selu-editor", &rel));
        let has_focus = ctx.memory(|m| m.has_focus(id));

        // Completion keys, taken before the text box sees them.
        let mut accept = false;
        if has_focus && let Some(c) = &mut self.completion {
            let n = c.items.len();
            ctx.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    c.selected = (c.selected + 1) % n;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    c.selected = (c.selected + n - 1) % n;
                }
                if i.consume_key(Modifiers::NONE, Key::Enter)
                    || i.consume_key(Modifiers::NONE, Key::Tab)
                {
                    accept = true;
                }
            });
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.completion = None;
            }
        }
        let force_complete =
            has_focus && ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Space));
        if accept && let Some(c) = self.completion.take() {
            let item = &c.items[c.selected];
            let buf = &mut self.open[active];
            let cursor = egui::TextEdit::load_state(&ctx, id)
                .and_then(|s| s.cursor.char_range())
                .map(|r| char_to_byte(&buf.text, r.primary.index.0))
                .unwrap_or(c.start);
            let end = cursor.max(c.start);
            buf.text.replace_range(c.start..end, &item.label);
            let new_cursor = byte_to_char(&buf.text, c.start + item.label.len());
            if let Some(mut state) = egui::TextEdit::load_state(&ctx, id) {
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(new_cursor),
                    )));
                state.store(&ctx, id);
            }
            self.edited_at = Some(now);
            self.locals_stale = true;
        }

        let file_problems: Vec<Problem> = self
            .problems
            .iter()
            .filter(|p| p.file.as_deref() == Some(rel.as_str()))
            .cloned()
            .collect();
        let api = self.api.clone();
        let font = FontId::monospace(self.font_size);
        let is_selu = rel.ends_with(".selu");
        let find_ranges = matches.clone();
        let current = current_match.clone();
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
            let text = text.as_str();
            let mut job = LayoutJob::default();
            let api_ref = api.as_deref();
            let is_api = |w: &str| api_ref.is_some_and(|a| a.is_api(w));
            let spans = if is_selu {
                highlight::spans(text, &is_api)
            } else {
                vec![(0..text.len(), Kind::Plain)]
            };
            let marks: Vec<(std::ops::Range<usize>, Severity)> = file_problems
                .iter()
                .map(|p| (problem_range(text, p), p.severity))
                .collect();
            let valid = |r: &std::ops::Range<usize>| r.end <= text.len();
            for (range, kind) in spans {
                let mut cuts = vec![range.start, range.end];
                for r in marks
                    .iter()
                    .map(|(r, _)| r)
                    .chain(find_ranges.iter())
                    .filter(|r| valid(r))
                {
                    for x in [r.start, r.end] {
                        if x > range.start && x < range.end && text.is_char_boundary(x) {
                            cuts.push(x);
                        }
                    }
                }
                cuts.sort_unstable();
                cuts.dedup();
                for w in cuts.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let mut format = TextFormat::simple(font.clone(), color(kind));
                    if let Some(r) = find_ranges.iter().find(|r| r.start <= a && b <= r.end) {
                        format.background = if current.as_ref() == Some(r) {
                            Color32::from_rgb(0x8a, 0x6a, 0x1f)
                        } else {
                            Color32::from_rgb(0x4a, 0x3d, 0x1c)
                        };
                    }
                    if let Some((_, sev)) = marks
                        .iter()
                        .find(|(m, _)| m.start <= a && b <= m.end && m.start < m.end)
                    {
                        let c = match sev {
                            Severity::Error => palette::ERROR,
                            Severity::Warning => palette::WARNING,
                        };
                        format.underline = Stroke::new(1.5, c);
                    }
                    job.append(&text[a..b], 0.0, format);
                }
            }
            job.wrap.max_width = f32::INFINITY;
            ui.ctx().fonts_mut(|f| f.layout_job(job))
        };

        let digits = (self.open[active].text.split('\n').count().max(1) as f32)
            .log10()
            .floor() as usize
            + 1;
        let char_w = ctx.fonts_mut(|f| f.glyph_width(&font, '0'));
        let gutter_w = (digits.max(3) as f32) * char_w + 30.0;
        let jump = self.jump.take();
        let jump_select = self.jump_select.take();
        let mut output = None;
        let buf = &mut self.open[active];
        egui::Frame::new().fill(palette::BG).show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt(("editor-scroll", &rel))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let line_bg = ui.painter().add(Shape::Noop);
                    ui.horizontal_top(|ui| {
                        ui.add_space(gutter_w);
                        let out = egui::TextEdit::multiline(&mut buf.text)
                            .id(id)
                            .code_editor()
                            .font(font.clone())
                            .frame(egui::Frame::NONE)
                            .margin(egui::Margin::symmetric(0, 6))
                            .desired_width(f32::INFINITY)
                            .desired_rows(40)
                            .layouter(&mut layouter)
                            .show(ui);
                        // Jumps from Problems, find, go-to.
                        let target = jump
                            .map(|at| (at, at))
                            .or_else(|| jump_select.clone().map(|r| (r.start, r.end)));
                        if let Some((a, b)) = target {
                            let ca = egui::text::CCursor::new(byte_to_char(&buf.text, a));
                            let cb = egui::text::CCursor::new(byte_to_char(&buf.text, b));
                            let rect = out
                                .galley
                                .pos_from_cursor(cb)
                                .translate(out.galley_pos.to_vec2());
                            ui.scroll_to_rect(rect.expand(60.0), Some(Align::Center));
                            let mut state = out.state.clone();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(ca, cb)));
                            state.store(ui.ctx(), id);
                            if jump.is_some() {
                                ui.ctx().memory_mut(|m| m.request_focus(id));
                            }
                        }
                        output = Some(out);
                    });
                    let Some(out) = &output else { return };
                    // Current line highlight and the gutter.
                    let clip = ui.clip_rect();
                    let painter = ui.painter();
                    let cursor_row_y = out.cursor_range.map(|r| {
                        out.galley
                            .pos_from_cursor(r.primary)
                            .translate(out.galley_pos.to_vec2())
                    });
                    if let Some(r) = cursor_row_y {
                        let line = egui::Rect::from_x_y_ranges(clip.x_range(), r.y_range());
                        painter.set(
                            line_bg,
                            egui::epaint::RectShape::filled(line, 0.0, palette::CURRENT_LINE),
                        );
                    }
                    let mut line_no = 1usize;
                    let mut starts_line = true;
                    let problem_lines: BTreeMap<u32, Severity> =
                        file_problems.iter().fold(BTreeMap::new(), |mut m, p| {
                            let e = m.entry(p.line).or_insert(p.severity);
                            if p.severity == Severity::Error {
                                *e = Severity::Error;
                            }
                            m
                        });
                    let cursor_line = out.cursor_range.map(|r| {
                        let b = char_to_byte(&buf.text, r.primary.index.0);
                        buf.text[..b].matches('\n').count() + 1
                    });
                    for row in &out.galley.rows {
                        if starts_line {
                            let rect = row.rect().translate(out.galley_pos.to_vec2());
                            if rect.bottom() >= clip.top() && rect.top() <= clip.bottom() {
                                let active_line = cursor_line == Some(line_no);
                                painter.text(
                                    egui::pos2(out.galley_pos.x - 14.0, rect.center().y),
                                    Align2::RIGHT_CENTER,
                                    line_no.to_string(),
                                    font.clone(),
                                    if active_line {
                                        palette::TEXT
                                    } else {
                                        palette::FAINT
                                    },
                                );
                                if let Some(sev) = problem_lines.get(&(line_no as u32)) {
                                    let c = match sev {
                                        Severity::Error => palette::ERROR,
                                        Severity::Warning => palette::WARNING,
                                    };
                                    painter.circle_filled(
                                        egui::pos2(
                                            out.galley_pos.x - gutter_w + 9.0,
                                            rect.center().y,
                                        ),
                                        3.5,
                                        c,
                                    );
                                }
                            }
                        }
                        starts_line = row.ends_with_newline;
                        if row.ends_with_newline {
                            line_no += 1;
                        }
                    }
                });
        });
        let Some(out) = output else { return };
        let buf = &self.open[active];

        // Cursor position for the status bar.
        self.cursor_pos = out.cursor_range.map(|r| {
            let b = char_to_byte(&buf.text, r.primary.index.0);
            let line = buf.text[..b].matches('\n').count() + 1;
            let col = buf.text[..b]
                .rsplit('\n')
                .next()
                .map_or(0, |l| l.chars().count())
                + 1;
            (line, col)
        });

        // After an edit: schedule a check and update completion.
        let typed = ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Paste(_)))
        });
        let changed = out.response.response.changed();
        let cursor_byte = out
            .cursor_range
            .map(|r| char_to_byte(&buf.text, r.primary.index.0));
        if changed {
            self.edited_at = Some(now);
            self.locals_stale = true;
        }
        if let Some(at) = cursor_byte
            && ((changed && typed) || force_complete || self.completion.is_some())
        {
            let (start, prefix, context) = complete::context_at(&buf.text, at);
            let wanted = force_complete
                || matches!(context, Context::Member(_) | Context::Event)
                || !prefix.is_empty();
            let items = if wanted && is_selu {
                complete::items(self.api.as_deref(), &self.locals, &context, &prefix, 40)
            } else {
                Vec::new()
            };
            let keep_open = self.completion.is_some() && !items.is_empty();
            if !items.is_empty() && ((changed && typed) || force_complete || keep_open) {
                let selected = self
                    .completion
                    .as_ref()
                    .map_or(0, |c| c.selected.min(items.len() - 1));
                self.completion = Some(Completion {
                    items,
                    selected,
                    start,
                });
            } else {
                self.completion = None;
            }
        }
        if !out.response.response.has_focus() && !accept {
            self.completion = None;
        }

        // Completion popup at the cursor.
        if let (Some(c), Some(at)) = (&self.completion, cursor_byte) {
            let cc = egui::text::CCursor::new(byte_to_char(&buf.text, at));
            let rect = out
                .galley
                .pos_from_cursor(cc)
                .translate(out.galley_pos.to_vec2());
            egui::Area::new(id.with("completion"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.left_bottom() + egui::vec2(-4.0, 4.0))
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.set_width(380.0);
                            for (i, item) in c.items.iter().enumerate().take(10) {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 22.0),
                                    Sense::hover(),
                                );
                                if i == c.selected {
                                    ui.painter().rect_filled(
                                        r,
                                        CornerRadius::same(4),
                                        palette::ACCENT_SOFT,
                                    );
                                }
                                let p = ui.painter();
                                let tag_color = match item.kind {
                                    complete::ItemKind::Function | complete::ItemKind::Method => {
                                        Color32::from_rgb(0xdc, 0xdc, 0xaa)
                                    }
                                    complete::ItemKind::Namespace => {
                                        Color32::from_rgb(0x4f, 0xc1, 0xff)
                                    }
                                    complete::ItemKind::Event => {
                                        Color32::from_rgb(0xd7, 0xba, 0x7d)
                                    }
                                    complete::ItemKind::Keyword => {
                                        Color32::from_rgb(0xc5, 0x86, 0xc0)
                                    }
                                    _ => palette::MUTED,
                                };
                                p.text(
                                    r.left_center() + Vec2::new(8.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    item.kind.tag(),
                                    FontId::monospace(11.0),
                                    tag_color,
                                );
                                p.text(
                                    r.left_center() + Vec2::new(44.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    &item.label,
                                    FontId::monospace(13.0),
                                    palette::TEXT,
                                );
                            }
                            if c.items.len() > 10 {
                                ui.label(
                                    egui::RichText::new(format!("  {} more", c.items.len() - 10))
                                        .small()
                                        .color(palette::FAINT),
                                );
                            }
                            let detail = &c.items[c.selected].detail;
                            if !detail.is_empty() {
                                crate::theme::rule(ui);
                                ui.add_space(2.0);
                                ui.label(
                                    egui::RichText::new(detail)
                                        .monospace()
                                        .size(12.0)
                                        .color(palette::MUTED),
                                );
                            }
                        });
                });
        }

        // Hover: problems and API signatures.
        if self.completion.is_none()
            && let Some(pos) = out.response.response.hover_pos()
        {
            let c = out.galley.cursor_from_pos(pos - out.galley_pos);
            let at = char_to_byte(&buf.text, c.index.0);
            let mut tip: Vec<(String, Color32)> = Vec::new();
            for p in &file_problems {
                let r = problem_range(&buf.text, p);
                if r.start <= at && at < r.end.max(r.start + 1) {
                    let col = match p.severity {
                        Severity::Error => palette::ERROR,
                        Severity::Warning => palette::WARNING,
                    };
                    tip.push((format!("{}  {}", p.message, p.code), col));
                    tip.extend(p.notes.iter().map(|n| (n.clone(), palette::MUTED)));
                }
            }
            if is_selu
                && let Some((_, dotted)) = highlight::dotted_at(&buf.text, at)
                && let Some(api) = &self.api
                && let Some(h) = api.hover(&dotted)
            {
                tip.push((h, palette::TEXT));
            }
            if !tip.is_empty() {
                out.response.response.clone().on_hover_ui_at_pointer(|ui| {
                    ui.set_max_width(520.0);
                    for (t, c) in &tip {
                        ui.label(egui::RichText::new(t).monospace().size(12.5).color(*c));
                    }
                });
            }
        }
    }
}
