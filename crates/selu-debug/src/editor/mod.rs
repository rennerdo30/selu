//! The code editor in Selu Studio: project files, tabs, syntax colours,
//! line numbers, errors while typing (squiggles and a Problems list),
//! completion from the game API and the project, and hover signatures.

pub mod check;
pub mod complete;
pub mod highlight;
pub mod view;

use check::{Checker, Job, Problem, Severity};
use complete::{ApiIndex, Item};
use eframe::egui::{self, Color32};
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
    find: Option<view::Find>,
    quick_open: Option<view::QuickOpen>,
    /// Collapsed folders in the Explorer.
    collapsed: BTreeSet<String>,
    /// 1-based line and column of the cursor, for the status bar.
    cursor_pos: Option<(usize, usize)>,
    /// Inline new-file, new-folder or rename field in the Explorer.
    edit: Option<view::Edit>,
    /// Asking before moving this path to the Recycle Bin.
    confirm_delete: Option<String>,
    /// The Explorer asked for Project Settings.
    settings_request: bool,
    /// Byte range to select in the active buffer (find).
    jump_select: Option<std::ops::Range<usize>>,
}

fn list_files(project: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if path.is_dir() {
                out.push(format!("{rel}/"));
                walk(root, &path, out);
            } else {
                out.push(rel);
            }
        }
    }
    // Files and folders (with a trailing `/`) the Explorer shows.
    let mut out = Vec::new();
    if !project.join("selu.toml").is_file() {
        walk(project, project, &mut out);
        return out;
    }
    if project.join("selu.toml").is_file() {
        out.push("selu.toml".to_string());
    }
    for top in ["src", "tests"] {
        if project.join(top).is_dir() {
            out.push(format!("{top}/"));
            walk(project, &project.join(top), &mut out);
        }
    }
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
    pub fn set_api(&mut self, manifest: &selu_manifest::Manifest) {
        self.api = Some(Arc::new(ApiIndex::from_manifest(manifest)));
        self.api_rx = None;
    }
    pub fn runtime_error(&mut self, message: &str) {
        let Some(project) = &self.project else { return };
        for (i, _) in message.match_indices(':') {
            let tail = &message[i + 1..];
            let Some((line, _)) = tail.split_once(':') else {
                continue;
            };
            let Ok(line) = line.parse::<u32>() else {
                continue;
            };
            let before = message[..i]
                .trim_start_matches('@')
                .trim_end_matches(['\"', ']']);
            if let Some(file) = self
                .files
                .iter()
                .filter(|f| !f.ends_with('/'))
                .find(|f| before.replace('\\', "/").ends_with(f.as_str()))
            {
                if project.join(file).is_file() {
                    self.problems.push(Problem {
                        severity: Severity::Error,
                        code: "Lua".into(),
                        message: message.into(),
                        file: Some(file.clone()),
                        line,
                        column: 1,
                        notes: vec![],
                    });
                }
                break;
            }
        }
    }

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
            find: None,
            quick_open: None,
            collapsed: BTreeSet::new(),
            cursor_pos: None,
            edit: None,
            confirm_delete: None,
            settings_request: false,
            jump_select: None,
        };
        if let Some(project) = editor.project.clone() {
            let first = editor
                .files
                .iter()
                .find(|f| f.ends_with("main.selu"))
                .or_else(|| {
                    editor.files.iter().find(|f| {
                        f.ends_with(".selu") || f.ends_with(".lua") || f.ends_with(".xml")
                    })
                })
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

    /// `[format]` settings from the project's `selu.toml`.
    fn format_config(&self) -> (selu_fmt::Options, bool) {
        let value = self
            .project
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p.join("selu.toml")).ok())
            .and_then(|t| toml::from_str::<toml::Value>(&t).ok());
        let format = value.as_ref().and_then(|v| v.get("format"));
        let indent = format
            .and_then(|f| f.get("indent"))
            .and_then(|v| v.as_integer())
            .filter(|n| (1..=8).contains(n))
            .map_or(2, |n| n as usize);
        let on_save = format
            .and_then(|f| f.get("on_save"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let width = format
            .and_then(|f| f.get("width"))
            .and_then(|v| v.as_integer())
            .filter(|n| (40..=400).contains(n))
            .map_or(100, |n| n as usize);
        (selu_fmt::Options { indent, width }, on_save)
    }

    /// Format buffer `i` in place; problems go to the messages.
    fn format_buffer(&mut self, i: usize, options: &selu_fmt::Options) {
        let Some(buf) = self.open.get_mut(i) else {
            return;
        };
        if !buf.rel.ends_with(".selu") {
            return;
        }
        match selu_fmt::format(&buf.text, options) {
            Ok(out) => {
                if out != buf.text {
                    buf.text = out;
                    self.locals_stale = true;
                }
            }
            Err(e) => self
                .messages
                .push(format!("cannot format {}: {e}", buf.rel)),
        }
    }

    /// Format Document (Shift+Alt+F).
    pub fn format_active(&mut self, now: f64) {
        let (options, _) = self.format_config();
        let before = self.open.get(self.active).map(|b| b.text.clone());
        self.format_buffer(self.active, &options);
        if self.open.get(self.active).map(|b| &b.text) != before.as_ref() {
            self.edited_at = Some(now);
        }
    }

    /// The project's configuration changed (Project Settings saved).
    pub fn config_changed(&mut self, now: f64) {
        self.edited_at = Some(now);
        self.messages.push("saved selu.toml".into());
    }

    pub fn project_dir(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    pub fn save_all(&mut self) {
        let (options, on_save) = self.format_config();
        if on_save {
            for i in 0..self.open.len() {
                if self.open[i].dirty() {
                    self.format_buffer(i, &options);
                }
            }
        }
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
        self.edited_at = self.checker.as_ref().map(|_| now);
    }

    /// Background work: API loading, check results, starting checks after
    /// a pause in typing, reloading files changed on disk.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.checker.is_none() {
            self.edited_at = None;
        }
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

    fn go_to(&mut self, i: usize) {
        let p = self.problems[i].clone();
        if let Some(f) = &p.file {
            self.open_file(f);
            if let Some(buf) = self.open.get(self.active) {
                self.jump = Some(offset_of(&buf.text, p.line, p.column));
            }
        }
    }

    /// Re-read the project listing (after creating, renaming or deleting).
    pub fn refresh_files(&mut self) {
        if let Some(project) = &self.project {
            self.files = list_files(project);
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
