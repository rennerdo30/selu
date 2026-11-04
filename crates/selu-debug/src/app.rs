//! The Selu Studio window (docs/SIMULATOR.md decisions 2, 6 and 12): an
//! egui app over the [`Worker`]. The GUI never touches Lua; it sends
//! commands and draws the snapshots and updates it gets back.
//!
//! Panels: toolbar (login, /reload, rebuild, time), frame tree, game
//! screen, inspector, and a bottom area with the console, the event trace
//! (with "fire an event") and the log.

use selu_debug::{Arg, Command, Update, Worker};
use selu_sim::{RegionSnapshot, SessionConfig, Snapshot, TextureFill, TraceRecord};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime};

use crate::theme::{caption, icon, icon_font, icon_text, palette};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect as ERect, Sense, Stroke,
    StrokeKind, Vec2,
};

/// What `selu-studio` was started with.
pub struct Options {
    pub addons: Vec<PathBuf>,
    pub target: Option<String>,
    /// The Selu project, for rebuild on save (`selu debug` passes it).
    pub project: Option<PathBuf>,
    /// The `selu` executable that builds the project.
    pub selu: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Print,
    Log,
    Error,
    Input,
    Build,
}

struct Line {
    kind: LineKind,
    text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Problems,
    Console,
    Events,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Explorer,
    Frames,
}

/// Where a screenshot was saved, or why it couldn't be.
type ShotResult = Result<PathBuf, String>;

const TRACE_KEEP: usize = 2_000;
const CONSOLE_KEEP: usize = 5_000;

pub struct Studio {
    opts: Options,
    worker: Worker,
    in_flight: usize,
    snapshot: Option<Snapshot>,
    generation: u64,
    console: VecDeque<Line>,
    trace: VecDeque<TraceRecord>,
    selected: Option<u32>,
    input: String,
    history: Vec<String>,
    event_name: String,
    event_args: String,
    playing: bool,
    last_tick: Option<f64>,
    speed: f64,
    auto_rebuild: bool,
    sources_stamp: Option<SystemTime>,
    last_poll: f64,
    tab: Tab,
    show_hidden: bool,
    shots: (Sender<ShotResult>, Receiver<ShotResult>),
    /// `SELU_STUDIO_SCREENSHOT=file.png`: save one screenshot once the
    /// session has settled, then close (for checks without a person).
    auto_shot: Option<(PathBuf, u32)>,
    editor: crate::editor::Editor,
    side: Side,
    side_open: bool,
    panel_open: bool,
    preview_open: bool,
    shortcuts_open: bool,
    settings: Option<crate::settings::SettingsWindow>,
    /// The window was asked to close with unsaved edits.
    confirm_close: bool,
}

impl Studio {
    pub fn new(opts: Options, ctx: &egui::Context) -> Studio {
        let worker = Worker::spawn(SessionConfig {
            target: opts.target.clone(),
            trace: true,
            ..SessionConfig::default()
        });
        let auto_rebuild = opts.project.is_some() && opts.selu.is_some();
        let (opts_project, opts_selu, opts_target) =
            (opts.project.clone(), opts.selu.clone(), opts.target.clone());
        let mut studio = Studio {
            sources_stamp: opts.project.as_deref().and_then(sources_stamp),
            opts,
            worker,
            in_flight: 0,
            snapshot: None,
            generation: 0,
            console: VecDeque::new(),
            trace: VecDeque::new(),
            selected: None,
            input: String::new(),
            history: Vec::new(),
            event_name: String::new(),
            event_args: String::new(),
            playing: false,
            last_tick: None,
            speed: 1.0,
            auto_rebuild,
            last_poll: 0.0,
            tab: Tab::Problems,
            show_hidden: false,
            shots: channel(),
            auto_shot: std::env::var_os("SELU_STUDIO_SCREENSHOT").map(|p| (PathBuf::from(p), 20)),
            side: if opts_project.is_some() {
                Side::Explorer
            } else {
                Side::Frames
            },
            side_open: true,
            panel_open: true,
            preview_open: true,
            shortcuts_open: false,
            settings: None,
            editor: crate::editor::Editor::new(opts_project, opts_selu, opts_target, ctx),
            confirm_close: false,
        };
        // For screenshots in checks: open Project Settings at start.
        if std::env::var_os("SELU_STUDIO_OPEN_SETTINGS").is_some() {
            studio.open_settings();
        }
        for dir in studio.opts.addons.clone() {
            studio.send(Command::LoadAddon { dir });
        }
        studio.send(Command::Login);
        studio.send(Command::Snapshot);
        studio
    }

    fn send(&mut self, command: Command) {
        self.in_flight += 1;
        self.worker.send(command);
    }

    /// A command from the user, followed by a fresh snapshot.
    fn act(&mut self, command: Command) {
        self.send(command);
        self.send(Command::Snapshot);
    }

    fn line(&mut self, kind: LineKind, text: impl Into<String>) {
        if self.console.len() == CONSOLE_KEEP {
            self.console.pop_front();
        }
        self.console.push_back(Line {
            kind,
            text: text.into(),
        });
    }

    fn drain(&mut self) {
        while let Ok(update) = self.worker.updates.try_recv() {
            match update {
                Update::Prints { lines } => {
                    for l in lines {
                        self.line(LineKind::Print, l);
                    }
                }
                Update::Logs { lines } => {
                    for l in lines {
                        let kind = if l.starts_with("error") || l.starts_with("callback error") {
                            LineKind::Error
                        } else {
                            LineKind::Log
                        };
                        self.line(kind, l);
                    }
                }
                Update::Trace { records } => {
                    for r in records {
                        if self.trace.len() == TRACE_KEEP {
                            self.trace.pop_front();
                        }
                        self.trace.push_back(r);
                    }
                }
                Update::Snapshot { snapshot } => self.snapshot = Some(*snapshot),
                Update::Error { message } => self.line(LineKind::Error, message),
                Update::Generation { generation } => {
                    self.generation = generation;
                    self.trace.clear();
                    self.selected = None;
                    self.line(
                        LineKind::Log,
                        format!("-- /reload (session {generation}) --"),
                    );
                }
                Update::Done { .. } => self.in_flight = self.in_flight.saturating_sub(1),
            }
        }
    }

    fn rebuild(&mut self) {
        let (Some(project), Some(selu)) = (self.opts.project.clone(), self.opts.selu.clone())
        else {
            self.line(
                LineKind::Error,
                "rebuild needs a project (start Studio with `selu debug`)",
            );
            return;
        };
        let mut cmd = std::process::Command::new(&selu);
        cmd.arg("build").arg(&project);
        if let Some(t) = &self.opts.target {
            cmd.arg("--target").arg(t);
        }
        self.line(LineKind::Build, format!("building {}", project.display()));
        match cmd.output() {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stderr).into_owned()
                    + &String::from_utf8_lossy(&out.stdout);
                for l in text.lines().filter(|l| !l.trim().is_empty()) {
                    let kind = if out.status.success() {
                        LineKind::Build
                    } else {
                        LineKind::Error
                    };
                    self.line(kind, l.to_string());
                }
                if out.status.success() {
                    self.act(Command::Reload);
                } else {
                    self.line(
                        LineKind::Error,
                        "build failed; the running session is unchanged",
                    );
                }
            }
            Err(e) => self.line(
                LineKind::Error,
                format!("cannot run {}: {e}", selu.display()),
            ),
        }
    }

    /// Ask egui for a screenshot of the window; the PNG is written on the
    /// callback and the result reported through `shots`.
    fn screenshot(&mut self, ctx: &egui::Context, path: PathBuf) {
        let tx = self.shots.0.clone();
        let repaint = ctx.clone();
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |image| {
                let [w, h] = image.size;
                let png = crate::png::encode(w as u32, h as u32, image.as_raw());
                let result = path
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&path, png))
                    .map(|()| path.clone())
                    .map_err(|e| format!("cannot save {}: {e}", path.display()));
                let _ = tx.send(result);
                repaint.request_repaint();
            }),
        ));
    }

    fn screenshot_path(&self) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let dir = self
            .opts
            .project
            .as_ref()
            .map_or_else(|| PathBuf::from("."), |p| p.join("dist").join(".selu"));
        dir.join(format!("studio-{stamp}.png"))
    }

    fn tick(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.shots.1.try_recv() {
            match result {
                Ok(path) => self.line(
                    LineKind::Build,
                    format!("screenshot saved: {}", path.display()),
                ),
                Err(e) => self.line(LineKind::Error, e),
            }
            if self.auto_shot.take().is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if let Some((path, frames)) = &mut self.auto_shot
            && self.snapshot.is_some()
            && self.in_flight == 0
            && self.editor.settled()
        {
            if *frames == 0 {
                let path = path.clone();
                self.auto_shot = Some((path.clone(), u32::MAX));
                self.screenshot(ctx, path);
            } else if *frames != u32::MAX {
                *frames -= 1;
            }
            ctx.request_repaint();
        }
        let now = ctx.input(|i| i.time);
        if self.playing {
            if let Some(last) = self.last_tick
                && self.in_flight == 0
            {
                let dt = ((now - last) * self.speed).clamp(0.0, 0.5);
                if dt > 0.0 {
                    self.act(Command::AdvanceTime { seconds: dt });
                }
            }
            if self.in_flight == 0 || self.last_tick.is_none() {
                self.last_tick = Some(now);
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            self.last_tick = None;
        }
        if self.auto_rebuild && now - self.last_poll > 1.0 {
            self.last_poll = now;
            if let Some(project) = self.opts.project.clone() {
                let stamp = sources_stamp(&project);
                if stamp.is_some() && stamp != self.sources_stamp {
                    self.sources_stamp = stamp;
                    self.rebuild();
                }
            }
        }
        if self.in_flight > 0 || self.auto_rebuild {
            ctx.request_repaint_after(Duration::from_millis(if self.in_flight > 0 {
                16
            } else {
                500
            }));
        }
    }
}

/// The newest modification time under the project's `src/` and `selu.toml`.
fn sources_stamp(project: &Path) -> Option<SystemTime> {
    fn walk(dir: &Path, best: &mut Option<SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, best);
            } else if let Ok(m) = entry.metadata().and_then(|m| m.modified()) {
                *best = Some(best.map_or(m, |b| b.max(m)));
            }
        }
    }
    let mut best = std::fs::metadata(project.join("selu.toml"))
        .and_then(|m| m.modified())
        .ok();
    walk(&project.join("src"), &mut best);
    best
}

fn region_label(r: &RegionSnapshot) -> String {
    let what = match &r.name {
        Some(n) => format!("{n} ({})", r.widget),
        None if r.id.index == 0 => "UIParent".to_string(),
        None => format!("{} #{}", r.widget, r.id.index),
    };
    match (&r.text, r.widget.as_str()) {
        (Some(t), "FontString") => format!("{what} \"{}\"", short(t, 24)),
        _ => what,
    }
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "..."
    }
}

fn parse_args(text: &str) -> Vec<Arg> {
    let trimmed = text.trim();
    if trimmed.starts_with('[')
        && let Ok(args) = serde_json::from_str::<Vec<Arg>>(trimmed)
    {
        return args;
    }
    trimmed
        .split_whitespace()
        .map(|w| match w {
            "true" => Arg::Bool(true),
            "false" => Arg::Bool(false),
            "nil" => Arg::Nil(()),
            _ => w
                .parse::<f64>()
                .map(Arg::Number)
                .unwrap_or_else(|_| Arg::String(w.trim_matches('"').to_string())),
        })
        .collect()
}

impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain();
        self.tick(&ctx);
        self.editor.poll(&ctx);
        for m in std::mem::take(&mut self.editor.messages) {
            self.line(LineKind::Build, m);
        }
        self.global_shortcuts(&ctx);
        self.close_guard(&ctx);
        self.editor.ui_quick_open(&ctx);
        if self.shortcuts_open {
            self.shortcuts_window(&ctx);
        }
        if let Some(w) = &mut self.settings {
            match w.ui(&ctx) {
                crate::settings::Outcome::Open => {}
                crate::settings::Outcome::Saved => {
                    self.settings = None;
                    let now = ctx.input(|i| i.time);
                    self.editor.config_changed(now);
                }
                crate::settings::Outcome::Closed => self.settings = None,
            }
        }

        let has_project = self.editor.has_project();
        egui::Panel::top("titlebar")
            .exact_size(38.0)
            .frame(
                egui::Frame::new()
                    .fill(palette::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show_separator_line(false)
            .show(ui, |ui| self.title_bar(ui));
        egui::Panel::bottom("statusbar")
            .exact_size(24.0)
            .frame(
                egui::Frame::new()
                    .fill(palette::ACCENT_SOFT)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show_separator_line(false)
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("activity")
            .exact_size(46.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(palette::BG))
            .show_separator_line(false)
            .show(ui, |ui| self.activity_bar(ui));
        if self.side_open {
            egui::Panel::left("sidebar")
                .resizable(true)
                .default_size(250.0)
                .size_range(160.0..=480.0)
                .frame(
                    egui::Frame::new()
                        .fill(palette::SIDEBAR)
                        .inner_margin(egui::Margin::symmetric(8, 10)),
                )
                .show(ui, |ui| match self.side {
                    Side::Explorer => self.editor.ui_explorer(ui),
                    Side::Frames => {
                        ui.horizontal(|ui| caption(ui, "Frames"));
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.tree(ui));
                    }
                });
        }
        if self.panel_open {
            egui::Panel::bottom("panel")
                .resizable(true)
                .default_size(210.0)
                .size_range(90.0..=600.0)
                .frame(
                    egui::Frame::new()
                        .fill(palette::PANEL)
                        .inner_margin(egui::Margin::symmetric(12, 6)),
                )
                .show(ui, |ui| self.bottom(ui));
        }
        if has_project && self.preview_open {
            egui::Panel::right("preview")
                .resizable(true)
                .default_size(560.0)
                .size_range(320.0..=1200.0)
                .frame(
                    egui::Frame::new()
                        .fill(palette::PANEL)
                        .inner_margin(egui::Margin::symmetric(10, 8)),
                )
                .show(ui, |ui| self.preview(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette::BG))
            .show(ui, |ui| {
                if has_project {
                    self.editor.ui_editor(ui);
                } else {
                    // No project: the game is the main view.
                    egui::Panel::right("inspector")
                        .resizable(true)
                        .default_size(300.0)
                        .frame(
                            egui::Frame::new()
                                .fill(palette::PANEL)
                                .inner_margin(egui::Margin::symmetric(10, 8)),
                        )
                        .show(ui, |ui| self.inspector(ui));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(palette::BG).inner_margin(12))
                        .show(ui, |ui| self.screen(ui));
                }
            });
    }
}

impl Studio {
    fn open_settings(&mut self) {
        if let Some(dir) = self.editor.project_dir() {
            match crate::settings::SettingsWindow::open(dir) {
                Ok(w) => self.settings = Some(w),
                Err(e) => self.line(LineKind::Error, e),
            }
        }
    }

    fn global_shortcuts(&mut self, ctx: &egui::Context) {
        self.editor.shortcuts(ctx);
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Comma)) {
            self.open_settings();
        }
        let (run, reload, build, panel, side, preview) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::F5),
                i.consume_key(Modifiers::COMMAND, Key::R),
                i.consume_key(Modifiers::COMMAND, Key::B),
                i.consume_key(Modifiers::COMMAND, Key::J),
                i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::E),
                i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::V),
            )
        });
        if run {
            self.playing = !self.playing;
        }
        if reload {
            self.act(Command::Reload);
        }
        if build {
            self.rebuild();
        }
        if panel {
            self.panel_open = !self.panel_open;
        }
        if side {
            self.side_open = !self.side_open;
        }
        if preview {
            self.preview_open = !self.preview_open;
        }
    }

    /// Closing with unsaved edits asks first.
    fn close_guard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && self.editor.any_dirty()
            && self.auto_shot.is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if !self.confirm_close {
            return;
        }
        egui::Modal::new(egui::Id::unique("unsaved")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(
                egui::RichText::new("Save changes before closing?")
                    .size(16.0)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Your changes will be lost if you don't save them.")
                    .color(palette::MUTED),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Save all").color(Color32::WHITE))
                            .fill(palette::ACCENT),
                    )
                    .clicked()
                {
                    self.editor.save_all();
                    self.confirm_close = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Don't save").clicked() {
                    self.editor.open.clear();
                    self.confirm_close = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Cancel").clicked() {
                    self.confirm_close = false;
                }
            });
        });
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Keyboard shortcuts")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Grid::new("keys")
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        for (k, what) in [
                            ("Ctrl+P", "Quick open a file"),
                            ("Ctrl+S", "Save all, rebuild and /reload"),
                            ("Ctrl+W", "Close the tab"),
                            ("Ctrl+F", "Find in the file"),
                            ("Shift+Alt+F", "Format the file"),
                            ("Ctrl+,", "Project settings"),
                            ("Ctrl+Space", "Complete a name"),
                            ("Ctrl+= / Ctrl+- / Ctrl+0", "Zoom the code"),
                            ("F5", "Run or pause the game clock"),
                            ("Ctrl+R", "/reload the addon"),
                            ("Ctrl+B", "Build"),
                            ("Ctrl+J", "Show or hide the bottom panel"),
                            ("Ctrl+Shift+E", "Show or hide the side bar"),
                            ("Ctrl+Shift+V", "Show or hide the game preview"),
                        ] {
                            ui.label(egui::RichText::new(k).monospace().color(palette::TEXT));
                            ui.label(egui::RichText::new(what).color(palette::MUTED));
                            ui.end_row();
                        }
                    });
            });
        if !open {
            self.shortcuts_open = false;
        }
    }

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.horizontal_centered(|ui| {
            ui.label(icon_text(icon::CODE, 15.0).color(palette::ACCENT));
            ui.add_space(2.0);
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add_enabled(
                            self.editor.has_project(),
                            egui::Button::new("Project Settings..."),
                        )
                        .clicked()
                    {
                        self.open_settings();
                    }
                    if ui
                        .add(egui::Button::new("Quick Open...").shortcut_text("Ctrl+P"))
                        .clicked()
                    {
                        self.editor.open_quick_open();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new("Save All").shortcut_text("Ctrl+S"))
                        .clicked()
                    {
                        self.editor.save_all();
                    }
                    if ui
                        .add(egui::Button::new("Close Tab").shortcut_text("Ctrl+W"))
                        .clicked()
                    {
                        let active = self.editor.active_index();
                        self.editor.close_tab(active);
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add(egui::Button::new("Format Document").shortcut_text("Shift+Alt+F"))
                        .clicked()
                    {
                        let now = ui.input(|i| i.time);
                        self.editor.format_active(now);
                    }
                    if ui
                        .add(egui::Button::new("Find").shortcut_text("Ctrl+F"))
                        .clicked()
                    {
                        self.editor.open_find();
                    }
                    ui.label(
                        egui::RichText::new("Undo Ctrl+Z, Redo Ctrl+Y")
                            .small()
                            .color(palette::FAINT),
                    );
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.side_open, "Side Bar");
                    ui.checkbox(&mut self.panel_open, "Bottom Panel");
                    if self.editor.has_project() {
                        ui.checkbox(&mut self.preview_open, "Game Preview");
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new("Zoom In").shortcut_text("Ctrl+="))
                        .clicked()
                    {
                        self.editor.zoom(1.0);
                    }
                    if ui
                        .add(egui::Button::new("Zoom Out").shortcut_text("Ctrl+-"))
                        .clicked()
                    {
                        self.editor.zoom(-1.0);
                    }
                });
                ui.menu_button("Run", |ui| {
                    let label = if self.playing { "Pause" } else { "Run" };
                    if ui
                        .add(egui::Button::new(label).shortcut_text("F5"))
                        .clicked()
                    {
                        self.playing = !self.playing;
                    }
                    if ui
                        .add(egui::Button::new("/reload").shortcut_text("Ctrl+R"))
                        .clicked()
                    {
                        self.act(Command::Reload);
                    }
                    let can_build = self.opts.project.is_some() && self.opts.selu.is_some();
                    if ui
                        .add_enabled(
                            can_build,
                            egui::Button::new("Build").shortcut_text("Ctrl+B"),
                        )
                        .clicked()
                    {
                        self.rebuild();
                    }
                    ui.add_enabled(
                        can_build,
                        egui::Checkbox::new(&mut self.auto_rebuild, "Rebuild on Save"),
                    );
                    ui.separator();
                    for (label, seconds) in [
                        ("Advance 0.1 s", 0.1),
                        ("Advance 1 s", 1.0),
                        ("Advance 10 s", 10.0),
                    ] {
                        if ui.button(label).clicked() {
                            self.act(Command::AdvanceTime { seconds });
                        }
                    }
                    ui.separator();
                    if ui.button("Save Screenshot").clicked() {
                        let path = self.screenshot_path();
                        self.screenshot(ui.ctx(), path);
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard Shortcuts").clicked() {
                        self.shortcuts_open = true;
                    }
                });
            });
        });
        // Centred run controls.
        let center = rect.center();
        let controls = egui::Rect::from_center_size(center, egui::vec2(300.0, 28.0));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(controls)
                .layout(egui::Layout::left_to_right(Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 4.0;
        let (icon, tip) = if self.playing {
            (icon::PAUSE, "Pause (F5)")
        } else {
            (icon::PLAY, "Run the game clock (F5)")
        };
        let play = egui::Button::new(icon_text(icon, 13.0).color(if self.playing {
            palette::WARNING
        } else {
            palette::OK
        }))
        .min_size(egui::vec2(32.0, 26.0));
        if child.add(play).on_hover_text(tip).clicked() {
            self.playing = !self.playing;
        }
        if child
            .add(egui::Button::new(icon_text(icon::REFRESH, 13.0)).min_size(egui::vec2(32.0, 26.0)))
            .on_hover_text("/reload (Ctrl+R)")
            .clicked()
        {
            self.act(Command::Reload);
        }
        let can_build = self.opts.project.is_some() && self.opts.selu.is_some();
        if child
            .add_enabled(
                can_build,
                egui::Button::new(icon_text(icon::BUILD, 13.0)).min_size(egui::vec2(32.0, 26.0)),
            )
            .on_hover_text("Build (Ctrl+B)")
            .clicked()
        {
            self.rebuild();
        }
        let time = self.snapshot.as_ref().map_or(0.0, |s| s.time);
        child.add_space(6.0);
        child.label(
            egui::RichText::new(format!("{time:.2} s"))
                .monospace()
                .color(palette::MUTED),
        );
        if self.in_flight > 0 {
            child.spinner();
        }
        // Project name on the right.
        let title = format!("{} — Selu Studio", self.editor.project_name());
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            title,
            FontId::proportional(12.5),
            palette::FAINT,
        );
    }

    fn activity_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            for (side, icon, tip) in [
                (Side::Explorer, icon::FILES, "Explorer (Ctrl+Shift+E)"),
                (Side::Frames, icon::FRAMES, "Frames"),
            ] {
                let active = self.side_open && self.side == side;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(46.0, 42.0), Sense::click());
                if active {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height())),
                        0.0,
                        palette::ACCENT,
                    );
                }
                let color = if active || resp.hovered() {
                    Color32::WHITE
                } else {
                    palette::FAINT
                };
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    icon,
                    icon_font(19.0),
                    color,
                );
                if resp.on_hover_text(tip).clicked() {
                    if self.side == side {
                        self.side_open = !self.side_open;
                    } else {
                        self.side = side;
                        self.side_open = true;
                    }
                }
            }
        });
        // Preview toggle at the bottom.
        if self.editor.has_project() {
            let bottom = ui.max_rect().bottom();
            let rect = egui::Rect::from_center_size(
                egui::pos2(ui.max_rect().center().x, bottom - 28.0),
                egui::vec2(46.0, 42.0),
            );
            let resp = ui.interact(rect, egui::Id::unique("preview-toggle"), Sense::click());
            let color = if self.preview_open || resp.hovered() {
                Color32::WHITE
            } else {
                palette::FAINT
            };
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                icon::PREVIEW,
                icon_font(18.0),
                color,
            );
            if resp.on_hover_text("Game preview (Ctrl+Shift+V)").clicked() {
                self.preview_open = !self.preview_open;
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let small = |t: String| {
                egui::RichText::new(t)
                    .size(12.0)
                    .color(Color32::from_rgb(0xe6, 0xe0, 0xff))
            };
            if let Some(t) = self
                .opts
                .target
                .clone()
                .or_else(|| self.editor.target_label())
            {
                ui.label(small(format!("◎ {t}")));
            }
            if self.editor.has_project() {
                let (e, w) = self.editor.counts();
                let (status, _) = self.editor.status();
                let text = if status == "checking..." {
                    "checking...".to_string()
                } else {
                    format!("⊗ {e}   ⚠ {w}")
                };
                if ui
                    .add(egui::Label::new(small(text)).sense(Sense::click()))
                    .on_hover_text("Problems")
                    .clicked()
                {
                    self.panel_open = true;
                    self.tab = Tab::Problems;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(small(format!("session {}", self.generation)));
                if let Some(s) = &self.snapshot {
                    ui.label(small(format!(
                        "game {:.2} s{}",
                        s.time,
                        if self.playing { " ▶" } else { "" }
                    )));
                }
                if self.editor.active_rel().is_some() {
                    ui.label(small("Selu".into()));
                    ui.label(small("UTF-8".into()));
                    if let Some((l, c)) = self.editor.cursor_line_col() {
                        ui.label(small(format!("Ln {l}, Col {c}")));
                    }
                }
            });
        });
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            caption(ui, "Game preview");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                for (label, seconds) in [("+10s", 10.0), ("+1s", 1.0)] {
                    if ui
                        .small_button(label)
                        .on_hover_text("Advance the game clock")
                        .clicked()
                    {
                        self.act(Command::AdvanceTime { seconds });
                    }
                }
                if ui
                    .small_button(icon_text(icon::CAMERA, 12.0))
                    .on_hover_text("Save a screenshot")
                    .clicked()
                {
                    let path = self.screenshot_path();
                    self.screenshot(ui.ctx(), path);
                }
            });
        });
        ui.add_space(6.0);
        let screen_h = (ui.available_width() * 9.0 / 16.0).min(ui.available_height() * 0.6);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), screen_h), Sense::hover());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        self.screen(&mut child);
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| self.inspector(ui));
    }

    fn tree(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_hidden, "show hidden");
        let Some(snap) = &self.snapshot else {
            ui.label("loading...");
            return;
        };
        let mut children: HashMap<Option<u32>, Vec<usize>> = HashMap::new();
        for (i, r) in snap.regions.iter().enumerate() {
            children
                .entry(r.parent.map(|p| p.index))
                .or_default()
                .push(i);
        }
        let mut clicked = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            #[allow(clippy::too_many_arguments)] // recursive tree row; a struct adds nothing
            fn row(
                ui: &mut egui::Ui,
                snap: &Snapshot,
                children: &HashMap<Option<u32>, Vec<usize>>,
                i: usize,
                depth: usize,
                selected: Option<u32>,
                show_hidden: bool,
                clicked: &mut Option<u32>,
            ) {
                let r = &snap.regions[i];
                if !show_hidden && !r.visible && depth > 0 {
                    return;
                }
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 12.0);
                    let mut text = egui::RichText::new(region_label(r));
                    if !r.visible {
                        text = text.weak();
                    }
                    if ui
                        .add(egui::Button::selectable(selected == Some(r.id.index), text))
                        .clicked()
                    {
                        *clicked = Some(r.id.index);
                    }
                });
                if let Some(kids) = children.get(&Some(r.id.index)) {
                    for &k in kids {
                        row(
                            ui,
                            snap,
                            children,
                            k,
                            depth + 1,
                            selected,
                            show_hidden,
                            clicked,
                        );
                    }
                }
            }
            for &root in children.get(&None).map(Vec::as_slice).unwrap_or(&[]) {
                row(
                    ui,
                    snap,
                    &children,
                    root,
                    0,
                    self.selected,
                    self.show_hidden,
                    &mut clicked,
                );
            }
        });
        if clicked.is_some() {
            self.selected = clicked;
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        caption(ui, "Inspector");
        ui.add_space(4.0);
        let Some(r) = self
            .snapshot
            .as_ref()
            .zip(self.selected)
            .and_then(|(s, i)| s.regions.get(i as usize))
        else {
            ui.label("Select a frame in the tree or on the screen.");
            return;
        };
        egui::Grid::new("fields")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                let mut field = |k: &str, v: String| {
                    ui.label(k);
                    ui.label(v);
                    ui.end_row();
                };
                field("type", r.widget.clone());
                field("name", r.name.clone().unwrap_or_else(|| "-".into()));
                field("id", format!("{}:{}", r.id.index, r.id.generation));
                field(
                    "parent",
                    r.parent.map_or("-".into(), |p| format!("#{}", p.index)),
                );
                field("shown", r.shown.to_string());
                field("visible", r.visible.to_string());
                field(
                    "rect",
                    r.rect.map_or("no position".into(), |x| {
                        format!(
                            "left {:.1}, bottom {:.1}\n{:.1} x {:.1}",
                            x.left, x.bottom, x.width, x.height
                        )
                    }),
                );
                field(
                    "size",
                    r.size.map_or("-".into(), |(w, h)| format!("{w} x {h}")),
                );
                field("alpha", format!("{:.2}", r.alpha));
                if let Some(t) = &r.texture {
                    field(
                        "texture",
                        match t {
                            TextureFill::File(f) => f.clone(),
                            TextureFill::Atlas(a) => format!("atlas {a}"),
                            TextureFill::Color(c) => {
                                format!("color {:.2} {:.2} {:.2} {:.2}", c[0], c[1], c[2], c[3])
                            }
                        },
                    );
                }
                if let Some(t) = &r.text {
                    field("text", t.clone());
                }
                if !r.scripts.is_empty() {
                    field("scripts", r.scripts.join("\n"));
                }
            });
        ui.add_space(8.0);
        ui.weak("Strata, levels, layers and scale arrive with the frames package (S1).");
    }

    fn screen(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snapshot else {
            ui.centered_and_justified(|ui| ui.label("starting the session..."));
            return;
        };
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let area = response.rect;
        let (uw, uh) = (snap.ui_width as f32, snap.ui_height as f32);
        let scale = (area.width() / uw).min(area.height() / uh);
        let origin = area.center() - Vec2::new(uw, uh) * scale / 2.0;
        let to_screen = |r: &selu_sim::Rect| {
            ERect::from_min_size(
                Pos2::new(
                    origin.x + r.left as f32 * scale,
                    origin.y + (uh - (r.bottom + r.height) as f32) * scale,
                ),
                Vec2::new(r.width as f32 * scale, r.height as f32 * scale),
            )
        };
        // The game world behind the UI.
        painter.rect_filled(
            ERect::from_min_size(origin, Vec2::new(uw, uh) * scale),
            0.0,
            Color32::from_rgb(28, 32, 40),
        );
        let visible = |r: &RegionSnapshot| r.visible || self.show_hidden;
        let fade = |r: &RegionSnapshot, c: Color32| {
            let a = r.alpha.clamp(0.0, 1.0) as f32 * if r.visible { 1.0 } else { 0.3 };
            c.gamma_multiply(a)
        };
        for r in snap.regions.iter().skip(1).filter(|r| visible(r)) {
            let Some(rect) = r.rect.as_ref().map(to_screen) else {
                continue;
            };
            match (&r.texture, r.widget.as_str()) {
                (Some(TextureFill::Color(c)), _) => {
                    let col = Color32::from_rgba_unmultiplied(
                        (c[0] * 255.0) as u8,
                        (c[1] * 255.0) as u8,
                        (c[2] * 255.0) as u8,
                        (c[3] * 255.0) as u8,
                    );
                    painter.rect_filled(rect, 0.0, fade(r, col));
                }
                (Some(TextureFill::File(name) | TextureFill::Atlas(name)), _) => {
                    painter.rect_filled(rect, 0.0, fade(r, Color32::from_rgb(60, 70, 95)));
                    let tail = name.rsplit(['\\', '/']).next().unwrap_or(name);
                    painter.with_clip_rect(rect.intersect(area)).text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        short(tail, 20),
                        FontId::proportional(10.0),
                        fade(r, Color32::LIGHT_GRAY),
                    );
                }
                (None, "FontString") => {
                    if let Some(text) = &r.text {
                        painter.text(
                            rect.left_top(),
                            Align2::LEFT_TOP,
                            text,
                            FontId::proportional((12.0 * scale).max(9.0)),
                            fade(r, Color32::from_rgb(255, 210, 0)),
                        );
                    }
                }
                (None, "Texture") => {}
                _ => {
                    painter.rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0, fade(r, Color32::from_gray(110))),
                        StrokeKind::Inside,
                    );
                }
            }
        }
        // Selection and hover outlines on top.
        let hit = |pos: Pos2| {
            snap.regions
                .iter()
                .skip(1)
                .rev()
                .filter(|r| visible(r))
                .find(|r| r.rect.as_ref().is_some_and(|x| to_screen(x).contains(pos)))
                .map(|r| r.id.index)
        };
        let hovered = response.hover_pos().and_then(hit);
        for (index, color) in [
            (hovered, Color32::from_rgb(120, 200, 255)),
            (self.selected, Color32::from_rgb(255, 220, 60)),
        ] {
            if let Some(rect) = index
                .and_then(|i| snap.regions.get(i as usize))
                .and_then(|r| r.rect.as_ref())
                .map(to_screen)
            {
                painter.rect_stroke(rect, 0.0, Stroke::new(2.0, color), StrokeKind::Outside);
            }
        }
        if let Some(i) = hovered
            && let Some(r) = snap.regions.get(i as usize)
        {
            response.clone().on_hover_text(region_label(r));
        }
        let unplaced = snap
            .regions
            .iter()
            .skip(1)
            .filter(|r| visible(r) && r.rect.is_none())
            .count();
        if unplaced > 0 {
            painter.text(
                area.left_bottom() + Vec2::new(6.0, -6.0),
                Align2::LEFT_BOTTOM,
                format!("{unplaced} region(s) without a position (no points set)"),
                FontId::proportional(11.0),
                Color32::GRAY,
            );
        }
        if response.clicked() {
            self.selected = response.interact_pointer_pos().and_then(hit);
        }
    }

    fn bottom(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            let (e, w) = self.editor.counts();
            let tabs = [
                (Tab::Problems, "PROBLEMS", e + w),
                (Tab::Console, "CONSOLE", 0),
                (Tab::Events, "EVENTS", 0),
            ];
            for (tab, label, count) in tabs {
                let active = self.tab == tab;
                let text = egui::RichText::new(label)
                    .size(11.5)
                    .strong()
                    .color(if active {
                        palette::TEXT
                    } else {
                        palette::MUTED
                    });
                let resp = ui.add(egui::Label::new(text).sense(Sense::click()));
                if count > 0 {
                    let badge = format!("{count}");
                    let g = ui.painter().layout_no_wrap(
                        badge,
                        FontId::proportional(10.5),
                        Color32::WHITE,
                    );
                    let r = egui::Rect::from_min_size(
                        resp.rect.right_center() + Vec2::new(5.0, -8.0),
                        egui::vec2(g.size().x + 10.0, 16.0),
                    );
                    ui.painter().rect_filled(
                        r,
                        8.0,
                        if e > 0 {
                            palette::ERROR
                        } else {
                            palette::WARNING
                        },
                    );
                    ui.painter()
                        .galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
                    ui.add_space(20.0);
                }
                if active {
                    let r = resp.rect;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(r.left(), r.bottom() + 3.0),
                            egui::pos2(r.right(), r.bottom() + 5.0),
                        ),
                        0.0,
                        palette::ACCENT,
                    );
                }
                if resp.clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("×").frame(false))
                    .on_hover_text("Hide the panel (Ctrl+J)")
                    .clicked()
                {
                    self.panel_open = false;
                }
                if self.tab == Tab::Console
                    && ui.add(egui::Button::new("Clear").frame(false)).clicked()
                {
                    self.console.clear();
                }
            });
        });
        ui.add_space(6.0);
        match self.tab {
            Tab::Problems => self.editor.ui_problems(ui),
            Tab::Console => self.console_tab(ui),
            Tab::Events => self.events_tab(ui),
        }
    }

    fn console_tab(&mut self, ui: &mut egui::Ui) {
        let entry = ui.horizontal(|ui| {
            ui.label(">");
            let edit = egui::TextEdit::singleline(&mut self.input)
                .hint_text("Lua code, or a slash command like /js")
                .desired_width(f32::INFINITY);
            ui.add(edit)
        });
        let submitted = entry.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if submitted && !self.input.trim().is_empty() {
            let text = std::mem::take(&mut self.input);
            self.line(LineKind::Input, format!("> {text}"));
            self.history.push(text.clone());
            if text.trim_start().starts_with('/') {
                self.act(Command::Slash { line: text });
            } else {
                self.act(Command::Eval { code: text });
            }
            entry.inner.request_focus();
        }
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for l in &self.console {
                    let color = match l.kind {
                        LineKind::Print => Color32::from_gray(230),
                        LineKind::Log => Color32::from_gray(140),
                        LineKind::Error => Color32::from_rgb(255, 110, 100),
                        LineKind::Input => Color32::from_rgb(120, 200, 255),
                        LineKind::Build => Color32::from_rgb(150, 210, 150),
                    };
                    ui.label(egui::RichText::new(&l.text).monospace().color(color));
                }
            });
    }

    fn events_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Fire");
            ui.add(
                egui::TextEdit::singleline(&mut self.event_name)
                    .hint_text("EVENT_NAME")
                    .desired_width(200.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.event_args)
                    .hint_text("args: player 42 true")
                    .desired_width(240.0),
            );
            if ui.button("Fire").clicked() && !self.event_name.trim().is_empty() {
                let event = self.event_name.trim().to_string();
                let args = parse_args(&self.event_args);
                self.act(Command::Fire { event, args });
            }
            if ui.button("Clear").clicked() {
                self.trace.clear();
            }
        });
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for r in &self.trace {
                    let text = format!(
                        "{:>9.3}  {:<14} {} {}",
                        r.time,
                        format!("{:?}", r.kind).to_lowercase(),
                        r.target,
                        r.args.join(", ")
                    );
                    ui.label(egui::RichText::new(text).monospace());
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use selu_sim::RegionId;

    #[test]
    fn event_args_parse_from_words_or_json() {
        assert_eq!(
            parse_args("player 42 true nil \"x\""),
            vec![
                Arg::String("player".into()),
                Arg::Number(42.0),
                Arg::Bool(true),
                Arg::Nil(()),
                Arg::String("x".into())
            ]
        );
        assert_eq!(
            parse_args(r#"["two words", 1]"#),
            vec![Arg::String("two words".into()), Arg::Number(1.0)]
        );
        assert!(parse_args("  ").is_empty());
    }

    #[test]
    fn labels_name_regions() {
        let r = RegionSnapshot {
            id: RegionId {
                index: 3,
                generation: 0,
            },
            parent: None,
            widget: "FontString".into(),
            name: None,
            rect: None,
            size: None,
            shown: true,
            visible: true,
            strata: None,
            level: None,
            layer: None,
            alpha: 1.0,
            texture: None,
            text: Some("Hello".into()),
            scripts: vec![],
            created_at: None,
        };
        assert_eq!(region_label(&r), "FontString #3 \"Hello\"");
    }
}
