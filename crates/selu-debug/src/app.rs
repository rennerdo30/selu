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

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect as ERect, Sense, Stroke, StrokeKind, Vec2,
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
enum View {
    Code,
    Game,
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
    view: View,
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
            view: if opts_project.is_some() {
                View::Code
            } else {
                View::Game
            },
            editor: crate::editor::Editor::new(opts_project, opts_selu, opts_target, ctx),
            confirm_close: false,
        };
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
        self.close_guard(&ctx);

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("bottom")
            .resizable(true)
            .default_size(200.0)
            .show(ui, |ui| self.bottom(ui));
        egui::Panel::left("side")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::CollapsingHeader::new("Files")
                        .default_open(true)
                        .show(ui, |ui| self.editor.ui_files(ui));
                    egui::CollapsingHeader::new("Frames")
                        .default_open(true)
                        .show(ui, |ui| self.tree(ui));
                });
            });
        if self.view == View::Game {
            egui::Panel::right("inspector")
                .resizable(true)
                .default_size(280.0)
                .show(ui, |ui| self.inspector(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Code => self.editor.ui_editor(ui),
            View::Game => self.screen(ui),
        });
    }
}

impl Studio {
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
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Some files have unsaved changes.");
                ui.horizontal(|ui| {
                    if ui.button("Save all and close").clicked() {
                        self.editor.save_all();
                        self.confirm_close = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Close without saving").clicked() {
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

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Selu Studio");
            ui.separator();
            ui.selectable_value(&mut self.view, View::Code, "Code");
            ui.selectable_value(&mut self.view, View::Game, "Game");
            ui.separator();
            if self.editor.has_project() {
                let dirty = self.editor.any_dirty();
                if ui
                    .add_enabled(dirty, egui::Button::new("Save"))
                    .on_hover_text("save all files (Ctrl+S); rebuilds and reloads the addon")
                    .clicked()
                {
                    self.editor.save_all();
                }
                let (status, col) = self.editor.status();
                if ui
                    .add(
                        egui::Label::new(egui::RichText::new(status).color(col))
                            .sense(Sense::click()),
                    )
                    .on_hover_text(self.editor.api_status.clone())
                    .clicked()
                {
                    self.tab = Tab::Problems;
                }
                ui.separator();
            }
            if ui.button("/reload").clicked() {
                self.act(Command::Reload);
            }
            let can_build = self.opts.project.is_some() && self.opts.selu.is_some();
            if ui
                .add_enabled(can_build, egui::Button::new("Rebuild"))
                .on_hover_text("build the project and /reload")
                .clicked()
            {
                self.rebuild();
            }
            ui.add_enabled(
                can_build,
                egui::Checkbox::new(&mut self.auto_rebuild, "rebuild on save"),
            );
            ui.separator();
            let play = if self.playing { "Pause" } else { "Play" };
            if ui
                .button(play)
                .on_hover_text("run simulated time")
                .clicked()
            {
                self.playing = !self.playing;
            }
            egui::ComboBox::from_id_salt("speed")
                .selected_text(format!("{}x", self.speed))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for s in [0.25, 0.5, 1.0, 2.0, 5.0] {
                        ui.selectable_value(&mut self.speed, s, format!("{s}x"));
                    }
                });
            for (label, seconds) in [("+0.1 s", 0.1), ("+1 s", 1.0), ("+10 s", 10.0)] {
                if ui.button(label).clicked() {
                    self.act(Command::AdvanceTime { seconds });
                }
            }
            if ui
                .button("Screenshot")
                .on_hover_text("save the window as a PNG")
                .clicked()
            {
                let path = self.screenshot_path();
                self.screenshot(ui.ctx(), path);
            }
            ui.separator();
            if let Some(s) = &self.snapshot {
                ui.label(format!("time {:.2} s", s.time));
            }
            ui.label(format!("session {}", self.generation));
            if let Some(t) = &self.opts.target {
                ui.label(t);
            }
            if self.in_flight > 0 {
                ui.spinner();
            }
        });
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
        ui.heading("Inspector");
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
            let n = self.editor.problems.len();
            ui.selectable_value(&mut self.tab, Tab::Problems, format!("Problems ({n})"));
            ui.selectable_value(&mut self.tab, Tab::Console, "Console");
            ui.selectable_value(&mut self.tab, Tab::Events, "Events");
        });
        ui.separator();
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
