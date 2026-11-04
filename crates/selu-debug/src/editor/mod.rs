//! The code editor in Selu Studio: project files, tabs, syntax colours,
//! line numbers, errors while typing (squiggles and a Problems list),
//! completion from the game API and the project, and hover signatures.

pub mod check;
pub mod complete;
pub mod highlight;

use check::{Checker, Job, Problem, Severity};
use complete::{ApiIndex, Context, Item};
use eframe::egui::{
    self, Align, Color32, FontId, Key, Modifiers, Stroke, TextFormat, text::LayoutJob,
};
use highlight::Kind;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::SystemTime;

pub struct Buffer {
    /// Project-relative path with forward slashes.
    pub rel: String,
    pub text: String,
    saved: String,
    stamp: Option<SystemTime>,
}

impl Buffer {
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }
}

struct Completion {
    items: Vec<Item>,
    selected: usize,
    /// Byte offset where the completed word starts.
    start: usize,
}

pub struct Editor {
    project: Option<PathBuf>,
    selu: Option<PathBuf>,
    target: Option<String>,
    files: Vec<String>,
    pub open: Vec<Buffer>,
    active: usize,
    pub problems: Vec<Problem>,
    /// `None` until the first check finishes.
    pub check_ok: Option<bool>,
    checker: Option<Checker>,
    check_seq: u64,
    checking: bool,
    edited_at: Option<f64>,
    api: Option<Arc<ApiIndex>>,
    api_rx: Option<Receiver<Result<ApiIndex, String>>>,
    pub api_status: String,
    locals: BTreeSet<String>,
    locals_stale: bool,
    completion: Option<Completion>,
    /// Byte offset to move the cursor to in the active buffer.
    jump: Option<usize>,
    font_size: f32,
    last_disk_poll: f64,
    pub messages: Vec<String>,
}

fn list_files(project: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|e| e == "selu")
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    if project.join("selu.toml").is_file() {
        out.push("selu.toml".to_string());
    }
    walk(project, &project.join("src"), &mut out);
    out
}

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn color(kind: Kind) -> Color32 {
    match kind {
        Kind::Plain => Color32::from_rgb(0x9c, 0xdc, 0xfe),
        Kind::Keyword => Color32::from_rgb(0xc5, 0x86, 0xc0),
        Kind::Type => Color32::from_rgb(0x4e, 0xc9, 0xb0),
        Kind::Constant => Color32::from_rgb(0x56, 0x9c, 0xd6),
        Kind::String => Color32::from_rgb(0xce, 0x91, 0x78),
        Kind::Number => Color32::from_rgb(0xb5, 0xce, 0xa8),
        Kind::Comment => Color32::from_rgb(0x6a, 0x99, 0x55),
        Kind::Call => Color32::from_rgb(0xdc, 0xdc, 0xaa),
        Kind::Api => Color32::from_rgb(0x4f, 0xc1, 0xff),
        Kind::Event => Color32::from_rgb(0xd7, 0xba, 0x7d),
        Kind::Punct => Color32::from_rgb(0xd4, 0xd4, 0xd4),
    }
}

/// Byte offset of 1-based `line`/`column` (columns count characters).
pub fn offset_of(text: &str, line: u32, column: u32) -> usize {
    let mut offset = 0;
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i + 1 == line as usize {
            return offset
                + l.char_indices()
                    .nth(column.saturating_sub(1) as usize)
                    .map_or(l.trim_end_matches('\n').len(), |(b, _)| b);
        }
        offset += l.len();
    }
    text.len()
}

/// The byte range a problem underlines: the word at its position, or one
/// character.
fn problem_range(text: &str, p: &Problem) -> std::ops::Range<usize> {
    let start = offset_of(text, p.line, p.column);
    if let Some((r, _)) = highlight::word_at(text, start)
        && r.start == start
    {
        return r;
    }
    let end = text[start..]
        .chars()
        .next()
        .filter(|c| *c != '\n')
        .map_or(start, |c| start + c.len_utf8());
    start..end
}

fn char_to_byte(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(b, _)| b)
}

fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

impl Editor {
    pub fn new(
        project: Option<PathBuf>,
        selu: Option<PathBuf>,
        target: Option<String>,
        ctx: &egui::Context,
    ) -> Editor {
        let mut editor = Editor {
            files: project.as_deref().map(list_files).unwrap_or_default(),
            checker: (project.is_some() && selu.is_some()).then(|| Checker::spawn(ctx.clone())),
            project,
            selu,
            target,
            open: Vec::new(),
            active: 0,
            problems: Vec::new(),
            check_ok: None,
            check_seq: 0,
            checking: false,
            edited_at: None,
            api: None,
            api_rx: None,
            api_status: String::new(),
            locals: BTreeSet::new(),
            locals_stale: true,
            completion: None,
            jump: None,
            font_size: 14.0,
            last_disk_poll: 0.0,
            messages: Vec::new(),
        };
        if let Some(project) = editor.project.clone() {
            let first = editor
                .files
                .iter()
                .find(|f| f.ends_with("main.selu"))
                .or_else(|| editor.files.iter().find(|f| f.ends_with(".selu")))
                .cloned();
            if let Some(f) = first {
                editor.open_file(&f);
            }
            // Load the game API for completion in the background.
            let target = editor
                .target
                .clone()
                .or_else(|| complete::project_target(&project));
            match (complete::find_manifests(&project), target) {
                (Some(dir), Some(target)) => {
                    let (tx, rx) = channel();
                    let repaint = ctx.clone();
                    editor.api_status = format!("loading the {target} API...");
                    std::thread::spawn(move || {
                        let _ = tx.send(complete::load_api(&dir, &target));
                        repaint.request_repaint();
                    });
                    editor.api_rx = Some(rx);
                }
                _ => {
                    editor.api_status =
                        "no manifests found: completion covers the project only".into()
                }
            }
            editor.request_check(0.0);
        }
        editor
    }

    /// No check pending or running and the API loaded (for screenshots).
    pub fn settled(&self) -> bool {
        !self.checking && self.edited_at.is_none() && self.api_rx.is_none()
    }

    pub fn has_project(&self) -> bool {
        self.project.is_some()
    }

    pub fn any_dirty(&self) -> bool {
        self.open.iter().any(Buffer::dirty)
    }

    pub fn open_file(&mut self, rel: &str) {
        if let Some(i) = self.open.iter().position(|b| b.rel == rel) {
            self.active = i;
            return;
        }
        let Some(project) = &self.project else { return };
        let path = project.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.open.push(Buffer {
                    rel: rel.to_string(),
                    saved: text.clone(),
                    text,
                    stamp: stamp(&path),
                });
                self.active = self.open.len() - 1;
                self.locals_stale = true;
            }
            Err(e) => self.messages.push(format!("cannot open {rel}: {e}")),
        }
    }

    pub fn save_all(&mut self) {
        for i in 0..self.open.len() {
            self.save(i);
        }
    }

    fn save(&mut self, i: usize) {
        let Some(project) = &self.project else { return };
        let buf = &mut self.open[i];
        if !buf.dirty() {
            return;
        }
        let path = project.join(&buf.rel);
        match std::fs::write(&path, &buf.text) {
            Ok(()) => {
                buf.saved = buf.text.clone();
                buf.stamp = stamp(&path);
                self.messages.push(format!("saved {}", buf.rel));
            }
            Err(e) => self.messages.push(format!("cannot save {}: {e}", buf.rel)),
        }
    }

    fn request_check(&mut self, now: f64) {
        self.edited_at = Some(now);
    }

    /// Background work: API loading, check results, starting checks after
    /// a pause in typing, reloading files changed on disk.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some(rx) = &self.api_rx
            && let Ok(result) = rx.try_recv()
        {
            self.api_rx = None;
            match result {
                Ok(api) => {
                    self.api_status = format!("API: {}", api.target);
                    self.api = Some(Arc::new(api));
                }
                Err(e) => self.api_status = format!("API not loaded: {e}"),
            }
        }
        if let Some(checker) = &self.checker {
            while let Ok(outcome) = checker.results.try_recv() {
                if outcome.seq == self.check_seq {
                    self.checking = false;
                }
                self.problems = outcome.problems;
                self.check_ok = Some(outcome.ok);
            }
        }
        if let Some(at) = self.edited_at
            && now - at > 0.5
            && let (Some(checker), Some(project), Some(selu)) =
                (&self.checker, &self.project, &self.selu)
        {
            self.edited_at = None;
            self.check_seq += 1;
            self.checking = true;
            checker.submit(Job {
                seq: self.check_seq,
                project: project.clone(),
                selu: selu.clone(),
                target: self.target.clone(),
                buffers: self
                    .open
                    .iter()
                    .filter(|b| b.dirty())
                    .map(|b| (b.rel.clone(), b.text.clone()))
                    .collect(),
            });
        }
        if self.edited_at.is_some() || self.checking || self.api_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Files changed outside Studio: reload clean buffers, list new files.
        if now - self.last_disk_poll > 1.0
            && let Some(project) = self.project.clone()
        {
            self.last_disk_poll = now;
            let files = list_files(&project);
            if files != self.files {
                self.files = files;
                self.request_check(now);
            }
            for buf in &mut self.open {
                let path = project.join(&buf.rel);
                let s = stamp(&path);
                if s != buf.stamp && !buf.dirty() {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        buf.saved = text.clone();
                        buf.text = text;
                        self.locals_stale = true;
                    }
                    buf.stamp = s;
                    self.edited_at = Some(now);
                }
            }
        }
    }

    pub fn status(&self) -> (String, Color32) {
        if self.checker.is_none() {
            return (
                "no checks (start Studio with selu debug)".into(),
                Color32::GRAY,
            );
        }
        if self.checking || self.edited_at.is_some() {
            return ("checking...".into(), Color32::GRAY);
        }
        let errors = self
            .problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .count();
        let warnings = self.problems.len() - errors;
        match (self.check_ok, errors, warnings) {
            (None, _, _) => ("checking...".into(), Color32::GRAY),
            (_, 0, 0) => ("no problems".into(), Color32::from_rgb(120, 200, 120)),
            (_, e, w) => (
                format!("{e} error(s), {w} warning(s)"),
                if e > 0 {
                    Color32::from_rgb(255, 110, 100)
                } else {
                    Color32::from_rgb(230, 190, 80)
                },
            ),
        }
    }

    pub fn ui_files(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            ui.weak("No Selu project. Start Studio with scripts\\studio.bat or `selu debug` to edit code.");
            return;
        }
        let mut open = None;
        for f in &self.files {
            let errors = self
                .problems
                .iter()
                .filter(|p| p.file.as_deref() == Some(f.as_str()) && p.severity == Severity::Error)
                .count();
            let dirty = self.open.iter().any(|b| &b.rel == f && b.dirty());
            let active = self.open.get(self.active).is_some_and(|b| &b.rel == f);
            let name = f.strip_prefix("src/").unwrap_or(f);
            let mut text = egui::RichText::new(format!("{}{name}", if dirty { "● " } else { "" }));
            if errors > 0 {
                text = text.color(Color32::from_rgb(255, 110, 100));
            }
            if ui.add(egui::Button::selectable(active, text)).clicked() {
                open = Some(f.clone());
            }
        }
        if let Some(f) = open {
            self.open_file(&f);
        }
    }

    pub fn ui_problems(&mut self, ui: &mut egui::Ui) {
        if self.problems.is_empty() {
            ui.weak(if self.check_ok.is_some() {
                "No problems."
            } else {
                "Checking..."
            });
            return;
        }
        let mut go = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, p) in self.problems.iter().enumerate() {
                    let (icon, col) = match p.severity {
                        Severity::Error => ("error", Color32::from_rgb(255, 110, 100)),
                        Severity::Warning => ("warning", Color32::from_rgb(230, 190, 80)),
                    };
                    let place = match &p.file {
                        Some(f) => format!(
                            "{}:{}:{}",
                            f.strip_prefix("src/").unwrap_or(f),
                            p.line,
                            p.column
                        ),
                        None => String::new(),
                    };
                    let response = ui.horizontal(|ui| {
                        ui.colored_label(col, icon);
                        ui.monospace(&p.code);
                        let r = ui.add(egui::Label::new(&p.message).sense(egui::Sense::click()));
                        ui.weak(place);
                        r
                    });
                    let mut r = response.inner;
                    if !p.notes.is_empty() {
                        r = r.on_hover_text(p.notes.join("\n"));
                    }
                    if r.clicked() {
                        go = Some(i);
                    }
                }
            });
        if let Some(i) = go {
            self.go_to(i);
        }
    }

    fn go_to(&mut self, i: usize) {
        let p = self.problems[i].clone();
        if let Some(f) = &p.file {
            self.open_file(f);
            if let Some(buf) = self.open.get(self.active) {
                self.jump = Some(offset_of(&buf.text, p.line, p.column));
            }
        }
    }

    pub fn ui_editor(&mut self, ui: &mut egui::Ui) {
        if self.open.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak(if self.project.is_some() {
                    "Open a file from the list on the left."
                } else {
                    "No Selu project open."
                })
            });
            return;
        }
        // Tabs.
        let mut close = None;
        ui.horizontal_wrapped(|ui| {
            for (i, b) in self.open.iter().enumerate() {
                let name = b.rel.rsplit('/').next().unwrap_or(&b.rel);
                let label = format!("{}{name}", if b.dirty() { "● " } else { "" });
                if ui
                    .add(egui::Button::selectable(i == self.active, label))
                    .on_hover_text(&b.rel)
                    .clicked()
                {
                    self.active = i;
                }
                if ui.small_button("x").on_hover_text("close").clicked() {
                    close = Some(i);
                }
                ui.separator();
            }
        });
        if let Some(i) = close {
            if self.open[i].dirty() {
                self.save(i);
            }
            self.open.remove(i);
            self.active = self.active.min(self.open.len().saturating_sub(1));
            if self.open.is_empty() {
                return;
            }
        }
        ui.separator();

        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save_all();
        }
        if self.locals_stale {
            let texts: Vec<&str> = self.open.iter().map(|b| b.text.as_str()).collect();
            self.locals = complete::local_words(&texts);
            self.locals_stale = false;
        }

        let active = self.active;
        let rel = self.open[active].rel.clone();
        let id = ui.scope_id().with(("selu-editor", &rel));
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
            self.request_check(now);
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
            for (range, kind) in spans {
                // Split the span where problem underlines start or end.
                let mut cuts = vec![range.start, range.end];
                for (m, _) in &marks {
                    for x in [m.start, m.end] {
                        if x > range.start && x < range.end {
                            cuts.push(x);
                        }
                    }
                }
                cuts.sort_unstable();
                cuts.dedup();
                for w in cuts.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let mark = marks
                        .iter()
                        .find(|(m, _)| m.start <= a && b <= m.end && m.start < m.end);
                    let mut format = TextFormat::simple(font.clone(), color(kind));
                    if let Some((_, sev)) = mark {
                        let c = match sev {
                            Severity::Error => Color32::from_rgb(255, 80, 80),
                            Severity::Warning => Color32::from_rgb(230, 190, 80),
                        };
                        format.underline = Stroke::new(1.5, c);
                        format.background = c.gamma_multiply(0.15);
                    }
                    job.append(&text[a..b], 0.0, format);
                }
            }
            job.wrap.max_width = f32::INFINITY;
            ui.ctx().fonts_mut(|f| f.layout_job(job))
        };

        let buf = &mut self.open[active];
        let lines = buf.text.split('\n').count().max(1);
        let gutter: String = (1..=lines).map(|n| format!("{n:>4}\n")).collect();
        let mut output = None;
        egui::ScrollArea::both()
            .id_salt(("editor-scroll", &rel))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(gutter.trim_end())
                                .font(font.clone())
                                .color(Color32::from_gray(90)),
                        )
                        .selectable(false),
                    );
                    let out = egui::TextEdit::multiline(&mut buf.text)
                        .id(id)
                        .code_editor()
                        .font(font.clone())
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .desired_width(f32::INFINITY)
                        .desired_rows(lines.max(30))
                        .layouter(&mut layouter)
                        .show(ui);
                    if let Some(at) = self.jump.take() {
                        let c = egui::text::CCursor::new(byte_to_char(&buf.text, at));
                        let rect = out
                            .galley
                            .pos_from_cursor(c)
                            .translate(out.galley_pos.to_vec2());
                        ui.scroll_to_rect(rect.expand(40.0), Some(Align::Center));
                        let mut state = out.state.clone();
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(c)));
                        state.store(ui.ctx(), id);
                        ui.ctx().memory_mut(|m| m.request_focus(id));
                    }
                    output = Some(out);
                });
            });
        let Some(out) = output else { return };

        // After an edit: schedule a check and update completion.
        let typed = ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Paste(_)))
        });
        let cursor_byte = out
            .cursor_range
            .map(|r| char_to_byte(&buf.text, r.primary.index.0));
        if out.response.response.changed() {
            self.edited_at = Some(now);
            self.locals_stale = true;
        }
        if let Some(at) = cursor_byte
            && ((out.response.response.changed() && typed)
                || force_complete
                || self.completion.is_some())
        {
            let (start, prefix, context) = complete::context_at(&buf.text, at);
            let wanted = force_complete
                || matches!(context, Context::Member(_) | Context::Event)
                || !prefix.is_empty();
            let items = if wanted {
                complete::items(self.api.as_deref(), &self.locals, &context, &prefix, 40)
            } else {
                Vec::new()
            };
            let keep_open = self.completion.is_some() && !items.is_empty();
            if !items.is_empty()
                && ((out.response.response.changed() && typed) || force_complete || keep_open)
            {
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
            egui::Area::new(ui.scope_id().with("selu-completion"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.left_bottom() + egui::vec2(0.0, 2.0))
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(260.0);
                        for (i, item) in c.items.iter().enumerate().take(12) {
                            let text = egui::RichText::new(format!(
                                "{:<3} {}",
                                item.kind.tag(),
                                item.label
                            ))
                            .monospace();
                            let _ = ui.add(egui::Button::selectable(i == c.selected, text));
                        }
                        if c.items.len() > 12 {
                            ui.weak(format!("{} more...", c.items.len() - 12));
                        }
                        let detail = &c.items[c.selected].detail;
                        if !detail.is_empty() {
                            ui.separator();
                            ui.label(egui::RichText::new(detail).monospace().size(12.0));
                        }
                    });
                });
        }

        // Hover: API signatures and problem messages.
        if self.completion.is_none()
            && let Some(pos) = out.response.response.hover_pos()
        {
            let c = out.galley.cursor_from_pos(pos - out.galley_pos);
            let at = char_to_byte(&buf.text, c.index.0);
            let mut tip: Vec<String> = Vec::new();
            for p in &file_problems {
                let r = problem_range(&buf.text, p);
                if r.start <= at && at <= r.end.max(r.start + 1) {
                    tip.push(format!(
                        "{} {}: {}",
                        match p.severity {
                            Severity::Error => "error",
                            Severity::Warning => "warning",
                        },
                        p.code,
                        p.message
                    ));
                    tip.extend(p.notes.iter().cloned());
                }
            }
            if is_selu
                && let Some((_, dotted)) = highlight::dotted_at(&buf.text, at)
                && let Some(api) = &self.api
                && let Some(h) = api.hover(&dotted)
            {
                tip.push(h);
            }
            if !tip.is_empty() {
                out.response.response.clone().on_hover_ui_at_pointer(|ui| {
                    for t in &tip {
                        ui.label(egui::RichText::new(t).monospace().size(12.0));
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_to_offsets() {
        let t = "ab\ncdé f\n";
        assert_eq!(offset_of(t, 1, 1), 0);
        assert_eq!(offset_of(t, 2, 1), 3);
        assert_eq!(offset_of(t, 2, 4), 7, "é is two bytes");
        assert_eq!(offset_of(t, 9, 1), t.len());
        let p = Problem {
            severity: Severity::Error,
            code: "X".into(),
            message: String::new(),
            file: None,
            line: 2,
            column: 1,
            notes: vec![],
        };
        assert_eq!(problem_range(t, &p), 3..7, "the word `cdé`");
        assert_eq!(byte_to_char(t, 7), 6);
        assert_eq!(char_to_byte(t, 6), 7);
    }
}
