//! The Selu Studio window (docs/SIMULATOR.md decisions 2, 6 and 12): an
//! egui app over the [`Worker`]. The GUI never touches Lua; it sends
//! commands and draws the snapshots and updates it gets back.
//!
//! Panels: toolbar (login, /reload, rebuild, time), frame tree, game
//! screen, inspector, and a bottom area with the console, the event trace
//! (with "fire an event") and the log.

use selu_debug::{Arg, Command, PreviewInput, Update, Worker};
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
    pub client_install: Option<PathBuf>,
    pub manifest: Option<selu_manifest::Manifest>,
    pub manifest_dir: Option<PathBuf>,
    pub addons: Vec<PathBuf>,
    pub target: Option<String>,
    /// The Selu project, for rebuild on save (`selu debug` passes it).
    pub project: Option<PathBuf>,
    /// The `selu` executable that builds the project.
    pub selu: Option<PathBuf>,
}

/// What the Open Project dialog shows.
enum OpenStatus {
    Failed(String),
}

/// The built addon folder in `dist/`: the one named like the project's addon,
/// else the first folder with a `.toc` (per-target layouts end in `-product`).
fn built_addon(project: &Path) -> Option<PathBuf> {
    if plain_addon(project) {
        return Some(project.to_path_buf());
    }
    let name = std::fs::read_to_string(project.join("selu.toml"))
        .ok()
        .and_then(|t| toml::from_str::<toml::Value>(&t).ok())
        .and_then(|v| v.get("addon")?.get("name")?.as_str().map(String::from));
    let mut found: Vec<PathBuf> = std::fs::read_dir(project.join("dist"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.'))
                && std::fs::read_dir(p).is_ok_and(|mut d| {
                    d.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == "toc")))
                })
        })
        .collect();
    found.sort();
    if let Some(name) = &name
        && let Some(exact) = found
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n == name.as_str()))
    {
        return Some(exact.clone());
    }
    found.into_iter().next()
}

/// One play-loop step: the new last tick, and the game seconds to advance.
/// Nothing advances while a reply is in flight; that time is counted once
/// the reply is back (at most half a second per step).
fn clock_step(
    last: Option<f64>,
    now: f64,
    in_flight: usize,
    speed: f64,
) -> (Option<f64>, Option<f64>) {
    match last {
        Some(last) if in_flight == 0 => {
            let dt = ((now - last) * speed).clamp(0.0, 0.5);
            (Some(now), (dt > 0.0).then_some(dt))
        }
        Some(last) => (Some(last), None),
        None => (Some(now), None),
    }
}

#[cfg(test)]
mod clock_tests {
    use super::clock_step;

    #[test]
    fn game_time_follows_wall_time() {
        // 60 frames a second for 10 seconds; every other frame waits on a reply.
        let (mut last, mut game) = (None, 0.0);
        for frame in 0..600 {
            let now = f64::from(frame) / 60.0;
            let in_flight = usize::from(frame % 2 == 1);
            let (l, dt) = clock_step(last, now, in_flight, 1.0);
            last = l;
            game += dt.unwrap_or(0.0);
        }
        assert!((game - 599.0 / 60.0).abs() < 0.05, "game time {game}");
    }
}

/// The project folder for a picked folder: the folder itself, its `selu`
/// subfolder, or its only direct subfolder that contains `selu.toml`.
fn find_project(picked: &std::path::Path) -> Option<PathBuf> {
    if picked.join("selu.toml").is_file() || plain_addon(picked) {
        return Some(picked.to_path_buf());
    }
    if picked.join("selu").join("selu.toml").is_file() {
        return Some(picked.join("selu"));
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(picked)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("selu.toml").is_file())
        .collect();
    found.sort();
    if found.len() == 1 { found.pop() } else { None }
}

fn plain_addon(dir: &Path) -> bool {
    !dir.join("selu.toml").is_file()
        && std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|e| {
                e.path()
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("toc"))
            })
        })
}

fn addon_manifest(
    dir: &Path,
    manifests: &Path,
    fallback: Option<&str>,
) -> Option<selu_manifest::Manifest> {
    let mut tocs: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("toc")))
        .collect();
    tocs.sort();
    let interfaces: Vec<u32> = tocs
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|t| {
            t.lines()
                .filter_map(|l| l.trim().strip_prefix("## Interface:").map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .flat_map(|s| {
            s.split(',')
                .filter_map(|v| v.trim().parse().ok())
                .collect::<Vec<_>>()
        })
        .collect();
    let all = selu_manifest::load_manifest_dir(manifests).ok()?;
    let fallback = fallback
        .and_then(|t| selu_manifest::TargetSpec::parse(t).ok())
        .and_then(|spec| selu_manifest::resolve(&spec, &all).ok());
    pick_plain_target(&all, &interfaces, fallback).cloned()
}

/// The manifest a plain addon runs against, from its TOC `## Interface:`
/// numbers (in the author's order). An exact interface match wins; when the
/// Studio setting's product is among the matches it is preferred, otherwise
/// the first listed interface decides. Without an exact match the setting
/// decides, then the nearest interface of the first listed number. No
/// product is assumed.
fn pick_plain_target<'a>(
    all: &'a [selu_manifest::Manifest],
    interfaces: &[u32],
    fallback: Option<&'a selu_manifest::Manifest>,
) -> Option<&'a selu_manifest::Manifest> {
    let real = || all.iter().filter(|m| !m.fixture);
    let newest = |interface: u32| {
        real()
            .filter(|m| m.interface == interface)
            .max_by_key(|m| m.build)
    };
    if let Some(preferred) = fallback.filter(|f| interfaces.contains(&f.interface)) {
        return Some(preferred);
    }
    if let Some(found) = interfaces.iter().find_map(|i| newest(*i)) {
        return Some(found);
    }
    fallback.or_else(|| {
        let first = *interfaces.first()?;
        real().min_by_key(|m| (m.interface.abs_diff(first), std::cmp::Reverse(m.build)))
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Print,
    Log,
    Error,
    Input,
    Build,
    /// A divider such as "/reload".
    Separator,
}

struct Line {
    kind: LineKind,
    text: String,
    /// Game time when the line arrived.
    time: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConsoleFilter {
    All,
    Output,
    Errors,
    Logs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventFilter {
    All,
    Events,
    Output,
    Timers,
    Errors,
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
    Scenario,
}

/// Messages from the background build.
enum BuildMsg {
    /// One output line; `true` when it came from stderr.
    Line(String, bool),
    /// The build finished; `true` on success.
    Done(bool),
}

/// Where a screenshot was saved, or why it couldn't be.
type ShotResult = Result<PathBuf, String>;

const TRACE_KEEP: usize = 2_000;
const CONSOLE_KEEP: usize = 5_000;

#[derive(Default)]
struct PreviewInputState {
    pointer: Option<(f64, f64)>,
    drag_start: Option<(f64, f64)>,
    modifiers: Modifiers,
}
fn preview_events(
    ui: &egui::Ui,
    response: &egui::Response,
    origin: Pos2,
    scale: f32,
    uh: f32,
    in_flight: usize,
    state: &mut PreviewInputState,
) -> Vec<PreviewInput> {
    let convert = |p: Pos2| {
        (
            ((p.x - origin.x) / scale) as f64,
            (uh - (p.y - origin.y) / scale) as f64,
        )
    };
    let mut events = Vec::new();
    // Geometry alone also contains clicks belonging to overlaid Studio widgets.
    // egui's response ownership respects occlusion and the active drag.
    let owns_pointer = response.hovered() || response.dragged();
    if response.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
        response.request_focus();
        state.drag_start = ui.input(|i| i.pointer.interact_pos()).map(|p| {
            (
                ((p.x - origin.x) / scale) as f64,
                (uh - (p.y - origin.y) / scale) as f64,
            )
        });
    }
    let focused = response.has_focus();
    let modifiers = if focused {
        ui.input(|i| i.modifiers)
    } else {
        Modifiers::NONE
    };
    for (key, was, now) in [
        ("LSHIFT", state.modifiers.shift, modifiers.shift),
        ("LCTRL", state.modifiers.ctrl, modifiers.ctrl),
        ("LALT", state.modifiers.alt, modifiers.alt),
    ] {
        if was != now {
            events.push(PreviewInput::Key {
                key: key.into(),
                down: now,
            });
        }
    }
    state.modifiers = modifiers;

    for event in ui.input(|i| i.events.clone()) {
        match event {
            egui::Event::PointerMoved(pos) => {
                let pointer = ((owns_pointer && response.rect.contains(pos))
                    || state.drag_start.is_some())
                .then(|| convert(pos));
                if (in_flight == 0 || state.drag_start.is_some()) && pointer != state.pointer {
                    let (x, y) = pointer.unwrap_or((-1., -1.));
                    events.push(PreviewInput::Hover { x, y });
                    state.pointer = pointer;
                }
            }
            egui::Event::PointerGone if state.pointer.is_some() && state.drag_start.is_none() => {
                events.push(PreviewInput::Hover { x: -1., y: -1. });
                state.pointer = None;
            }
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                ..
            } if (pressed && owns_pointer && response.rect.contains(pos))
                || (!pressed && focused) =>
            {
                let name = match button {
                    egui::PointerButton::Primary => "LeftButton",
                    egui::PointerButton::Secondary => "RightButton",
                    egui::PointerButton::Middle => "MiddleButton",
                    _ => continue,
                };
                let (x, y) = convert(pos);
                events.push(PreviewInput::MouseButton {
                    x,
                    y,
                    button: name.into(),
                    down: pressed,
                });
                state.pointer = Some((x, y));
                if button == egui::PointerButton::Primary {
                    state.drag_start = pressed.then_some((x, y));
                }
            }
            egui::Event::MouseWheel { delta, .. } if response.hovered() && delta.y != 0. => {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                    let (x, y) = convert(pos);
                    events.push(PreviewInput::Wheel {
                        x,
                        y,
                        delta: f64::from(delta.y.signum()),
                    });
                }
            }
            _ => {}
        }
    }
    // Deliver the newest skipped hover once the worker catches up.
    if in_flight == 0
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
    {
        let pointer = ((owns_pointer && response.rect.contains(pos)) || state.drag_start.is_some())
            .then(|| convert(pos));
        if pointer != state.pointer {
            let (x, y) = pointer.unwrap_or((-1., -1.));
            events.push(PreviewInput::Hover { x, y });
            state.pointer = pointer;
        }
    }
    if focused {
        for event in ui.input(|i| i.events.clone()) {
            match event {
                egui::Event::Text(text) => events.push(PreviewInput::Text(text)),
                egui::Event::Key { key, pressed, .. } => events.push(PreviewInput::Key {
                    key: match key {
                        Key::ArrowUp => "UP".into(),
                        Key::ArrowDown => "DOWN".into(),
                        Key::ArrowLeft => "LEFT".into(),
                        Key::ArrowRight => "RIGHT".into(),
                        _ => format!("{key:?}").to_ascii_uppercase(),
                    },
                    down: pressed,
                }),
                _ => {}
            }
        }
    }
    events
}

pub struct Studio {
    opts: Options,
    worker: Worker,
    in_flight: usize,
    snapshot: Option<Snapshot>,
    generation: u64,
    console: VecDeque<Line>,
    trace: VecDeque<TraceRecord>,
    selected: Option<u32>,
    preview_input: PreviewInputState,
    input: String,
    history: Vec<String>,
    history_pos: Option<usize>,
    console_filter: ConsoleFilter,
    console_search: String,
    event_filter: EventFilter,
    event_search: String,
    logo: Option<egui::TextureHandle>,
    assets: selu_sim::assets::Resolver,
    textures: HashMap<String, Result<egui::TextureHandle, String>>,
    glyphs: crate::text_render::GlyphCache,
    game_settings: crate::game::Settings,
    /// File > Open Project in progress: the background build's answer.
    /// The next successful build loads the newly opened project.
    load_after_build: bool,
    open_status: Option<OpenStatus>,
    game_clients: Vec<crate::game::Client>,
    game_scanned: bool,
    games_open: bool,
    game_new_root: String,
    /// Copy into this client after the running build succeeds.
    pending_deploy: Option<(String, crate::game::Client)>,
    confirm_replace: Option<(String, crate::game::Client, PathBuf)>,
    /// Run in Game's "which version?" dialog, with the selected client.
    game_picker: Option<usize>,
    /// The running background build, if any.
    build: Option<Receiver<BuildMsg>>,
    build_failed: bool,
    repaint: Option<egui::Context>,
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
    scenario: selu_sim::world::Scenario,
    scenario_token: String,
    scenario_unit: selu_sim::world::Unit,
    scenario_name: String,
    scenario_bag: String,
    scenario_bag_id: i32,
    scenario_status: String,
    saved_choice: Vec<String>,
    saved_lists: [Vec<String>; 3],
    /// The WTF folder the account list was read from; a new folder rescans.
    saved_scanned: Option<PathBuf>,
    /// Why a saved-variables list is empty, shown under the pickers.
    saved_scan_note: String,
    saved_status: String,
    saved_identity: Option<(String, String)>,
    saved_seed: Option<(
        PathBuf,
        std::collections::BTreeMap<String, selu_sim::saved::SavedValue>,
    )>,
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
    pub fn new(mut opts: Options, ctx: &egui::Context) -> Studio {
        if let Some(dir) = opts.project.clone().filter(|p| plain_addon(p)) {
            let manifests = opts
                .manifest_dir
                .clone()
                .or_else(|| crate::editor::complete::find_manifests(&dir));
            opts.manifest = manifests
                .as_deref()
                .and_then(|m| addon_manifest(&dir, m, opts.target.as_deref()));
            opts.target = opts
                .manifest
                .as_ref()
                .map(selu_manifest::Manifest::target_string)
                .or(opts.target);
            opts.addons = vec![dir];
            opts.selu = None;
        }
        let mut assets = selu_sim::assets::Resolver::default();
        if let Some(root) = &opts.client_install {
            let target = opts
                .manifest
                .as_ref()
                .map(selu_manifest::Manifest::target_string)
                .or_else(|| opts.target.clone());
            if let Some(target) = target {
                let _ = assets.open_client(root, target.split('@').next().unwrap_or_default());
            }
        }
        for root in &opts.addons {
            assets.add_addon(root.clone());
        }
        let worker = Worker::spawn(SessionConfig {
            client_install: opts.client_install.clone(),
            manifest: opts.manifest.clone(),
            target: opts.target.clone(),
            trace: true,
            ..SessionConfig::default()
        });
        let auto_rebuild = opts.project.is_some()
            && (opts.selu.is_some() || opts.project.as_deref().is_some_and(plain_addon));
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
            preview_input: PreviewInputState::default(),
            input: String::new(),
            history: Vec::new(),
            history_pos: None,
            console_filter: ConsoleFilter::All,
            console_search: String::new(),
            event_filter: EventFilter::All,
            event_search: String::new(),
            build: None,
            build_failed: false,
            game_settings: crate::game::Settings::load(),
            load_after_build: false,
            open_status: None,
            game_clients: Vec::new(),
            game_scanned: false,
            games_open: false,
            game_new_root: String::new(),
            pending_deploy: None,
            confirm_replace: None,
            game_picker: None,
            repaint: Some(ctx.clone()),
            logo: crate::theme::logo_image()
                .map(|img| ctx.load_texture("selu-logo", img, egui::TextureOptions::LINEAR)),
            assets,
            textures: HashMap::new(),
            glyphs: Default::default(),
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
            scenario: Default::default(),
            scenario_token: "target".into(),
            scenario_unit: Default::default(),
            scenario_name: "rare-test".into(),
            scenario_bag: String::new(),
            scenario_bag_id: 0,
            scenario_status: String::new(),
            saved_choice: vec![String::new(); 3],
            saved_lists: Default::default(),
            saved_scanned: None,
            saved_scan_note: String::new(),
            saved_status: String::new(),
            saved_identity: None,
            saved_seed: None,
            side: if std::env::var_os("SELU_STUDIO_SCENARIO").is_some() {
                Side::Scenario
            } else if opts_project.is_some() {
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
        // Started with a project that has not built yet (`selu debug` after a
        // failed build): open it anyway, build here, load once it builds.
        if let Some(m) = &studio.opts.manifest {
            studio.editor.set_api(m);
        }
        if studio.auto_rebuild && studio.opts.addons.is_empty() {
            studio.load_after_build = true;
            studio.rebuild();
        }
        // For screenshots in checks: open Run in Game's version picker.
        if std::env::var_os("SELU_STUDIO_OPEN_PICKER").is_some() {
            studio.run_in_game();
        }
        // For screenshots in checks: open the Game Installs window.
        if std::env::var_os("SELU_STUDIO_OPEN_GAMES").is_some() {
            studio.games_open = true;
        }
        // For screenshots in checks: pick the bottom tab.
        match std::env::var("SELU_STUDIO_TAB").as_deref() {
            Ok("console") => studio.tab = Tab::Console,
            Ok("events") => studio.tab = Tab::Events,
            _ => {}
        }
        // For screenshots in checks: open Project Settings at start.
        if std::env::var_os("SELU_STUDIO_OPEN_SETTINGS").is_some() {
            studio.open_settings();
        }
        studio.queue_default_ui();
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
        let time = self.snapshot.as_ref().map_or(0.0, |s| s.time);
        self.console.push_back(Line {
            kind,
            text: text.into(),
            time,
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
                        if kind == LineKind::Error {
                            self.editor.runtime_error(&l);
                        }
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
                Update::Scenario { state } => self.scenario = state,
                Update::Snapshot { snapshot } => self.snapshot = Some(*snapshot),
                Update::Error { message } => {
                    self.editor.runtime_error(&message);
                    self.line(LineKind::Error, message);
                }
                Update::Generation { generation } => {
                    self.generation = generation;
                    self.trace.clear();
                    self.selected = None;
                    self.line(
                        LineKind::Separator,
                        format!("/reload · session {generation}"),
                    );
                }
                Update::Done { .. } => self.in_flight = self.in_flight.saturating_sub(1),
            }
        }
    }

    /// Start `selu build` on a background thread. Output streams into the
    /// console as it arrives; a successful build reloads the addon. The UI
    /// thread never waits for it.
    fn rebuild(&mut self) {
        if self.opts.project.as_deref().is_some_and(plain_addon) {
            self.editor.problems.clear();
            self.act(Command::Reload);
            return;
        }
        if self.build.is_some() {
            return; // one build at a time
        }
        let (Some(project), Some(selu)) = (self.opts.project.clone(), self.opts.selu.clone())
        else {
            self.line(
                LineKind::Error,
                "building needs a project (start Studio with `selu debug`)",
            );
            return;
        };
        let target = self.opts.target.clone();
        let manifest_dir = self.opts.manifest_dir.clone();
        let (tx, rx) = channel::<BuildMsg>();
        let repaint = self.repaint.clone();
        self.line(LineKind::Build, format!("building {}", project.display()));
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let send = |m: BuildMsg| {
                let _ = tx.send(m);
                if let Some(ctx) = &repaint {
                    ctx.request_repaint();
                }
            };
            let mut cmd = std::process::Command::new(&selu);
            cmd.arg("build")
                .arg(&project)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            if let Some(t) = &target {
                cmd.arg("--target").arg(t);
            }
            if let Some(dir) = manifest_dir {
                cmd.arg("--manifest-dir").arg(dir);
            }
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    send(BuildMsg::Line(
                        format!("cannot run {}: {e}", selu.display()),
                        true,
                    ));
                    send(BuildMsg::Done(false));
                    return;
                }
            };
            // Read stderr on its own thread so neither pipe can fill up.
            let stderr = child.stderr.take().map(|err| {
                let tx = tx.clone();
                let repaint = repaint.clone();
                std::thread::spawn(move || {
                    for line in BufReader::new(err).lines().map_while(Result::ok) {
                        if !line.trim().is_empty() {
                            let _ = tx.send(BuildMsg::Line(line, true));
                            if let Some(ctx) = &repaint {
                                ctx.request_repaint();
                            }
                        }
                    }
                })
            });
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if !line.trim().is_empty() {
                        send(BuildMsg::Line(line, false));
                    }
                }
            }
            if let Some(h) = stderr {
                let _ = h.join();
            }
            let ok = child.wait().map(|s| s.success()).unwrap_or(false);
            send(BuildMsg::Done(ok));
        });
        self.build = Some(rx);
        self.build_failed = false;
    }

    /// Take what the background build has produced so far.
    fn poll_build(&mut self) {
        let Some(rx) = &self.build else { return };
        let mut msgs = Vec::new();
        while let Ok(m) = rx.try_recv() {
            msgs.push(m);
        }
        for m in msgs {
            match m {
                // stderr carries diagnostics; mark them as errors only when
                // the build fails (warnings print there too).
                BuildMsg::Line(text, from_stderr) => {
                    let kind = if from_stderr && text.contains("error") {
                        LineKind::Error
                    } else {
                        LineKind::Build
                    };
                    self.line(kind, text);
                }
                BuildMsg::Done(ok) => {
                    self.build = None;
                    self.build_failed = !ok;
                    if ok && std::mem::take(&mut self.load_after_build) {
                        self.load_built_project();
                    } else if ok {
                        self.act(Command::Reload);
                        if let Some((flavour, client)) = self.pending_deploy.take() {
                            self.deploy(&flavour, &client, false);
                        }
                    } else {
                        self.pending_deploy = None;
                        self.line(
                            LineKind::Error,
                            "build failed; the running addon is unchanged",
                        );
                    }
                    return;
                }
            }
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
        if !self.sim_ready() {
            self.playing = false;
        }
        if self.playing {
            // Game time follows wall time. The tick moves forward when an
            // advance is sent, so time that passed while a reply was in flight
            // is counted once on the next advance, not again on every frame.
            let (last, dt) = clock_step(self.last_tick, now, self.in_flight, self.speed);
            self.last_tick = last;
            if let Some(seconds) = dt {
                self.act(Command::AdvanceTime { seconds });
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
    if plain_addon(project) {
        walk(project, &mut best);
    } else {
        walk(&project.join("src"), &mut best);
    }
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
        self.poll_build();
        if self.build.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.tick(&ctx);
        self.editor.poll(&ctx);
        for m in std::mem::take(&mut self.editor.messages) {
            self.line(LineKind::Build, m);
        }
        self.global_shortcuts(&ctx);
        self.close_guard(&ctx);
        self.edge_resize(&ctx);
        self.editor.ui_quick_open(&ctx);
        self.ui_games(&ctx);
        self.ui_confirm_replace(&ctx);
        self.ui_game_picker(&ctx);
        self.ui_open_status(&ctx);
        if self.editor.take_settings_request() {
            self.open_settings();
        }
        if self.shortcuts_open {
            self.shortcuts_window(&ctx);
        }
        if let Some(w) = &mut self.settings {
            match w.ui(&ctx) {
                crate::settings::Outcome::Open => {}
                crate::settings::Outcome::LockUpdated => {
                    let now = ctx.input(|i| i.time);
                    self.editor.config_changed(now);
                }
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
                    Side::Scenario => self.scenario_ui(ui),
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

    /// The flavour the project is tested on in game: `--target`, else the
    /// first target in `selu.toml`.
    fn game_flavour(&self) -> Option<String> {
        let target = self.opts.target.clone().or_else(|| {
            self.opts
                .project
                .as_deref()
                .and_then(crate::editor::complete::project_target)
        })?;
        Some(
            target
                .split('@')
                .next()
                .unwrap_or(&target)
                .trim()
                .to_string(),
        )
    }

    fn ensure_clients(&mut self) {
        if !self.game_scanned {
            self.game_clients = crate::game::detect(&self.game_settings);
            self.game_scanned = true;
        }
    }

    /// Run in Game (Ctrl+F5): build, copy into the client's AddOns, start it.
    /// Every flavour the project builds for: `--target`, else `selu.toml`'s
    /// targets, in order.
    fn project_flavours(&self) -> Vec<String> {
        let targets: Vec<String> = match &self.opts.target {
            Some(t) => t.split(',').map(|s| s.trim().to_string()).collect(),
            None => self
                .opts
                .project
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p.join("selu.toml")).ok())
                .and_then(|t| toml::from_str::<toml::Value>(&t).ok())
                .and_then(|v| {
                    v.get("addon")?.get("targets")?.as_array().map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                })
                .unwrap_or_default(),
        };
        let mut out: Vec<String> = Vec::new();
        for t in targets {
            let f = selu_manifest::product(t.split('@').next().unwrap_or(&t).trim()).to_string();
            if !f.is_empty() && !out.contains(&f) {
                out.push(f);
            }
        }
        out
    }

    /// Run in Game (Ctrl+F5): ask which game version, then build, copy into
    /// that client's AddOns and start it.
    fn run_in_game(&mut self) {
        if self.opts.project.is_none() || self.opts.selu.is_none() {
            self.line(
                LineKind::Error,
                "Run in Game needs a project (start Studio with `selu debug`)",
            );
            return;
        }
        if self.build.is_some() {
            self.line(LineKind::Log, "a build is already running");
            return;
        }
        self.ensure_clients();
        // Preselect the last choice, else the first target's usual client.
        let flavours = self.project_flavours();
        let last = self.game_settings.chosen.get("__last_run").and_then(|f| {
            self.game_clients
                .iter()
                .position(|c| c.folder.to_string_lossy() == *f)
        });
        let default = last.or_else(|| {
            flavours.iter().find_map(|f| {
                let c = crate::game::pick(&self.game_clients, f, &self.game_settings)?;
                self.game_clients.iter().position(|x| x.folder == c.folder)
            })
        });
        self.game_picker = Some(default.unwrap_or(0));
    }

    /// Start a chosen client: remember it, build, then copy and launch.
    fn start_in_game(&mut self, client: crate::game::Client) {
        let Some(flavour) = client.flavour.clone() else {
            return;
        };
        let key = client.folder.to_string_lossy().into_owned();
        self.game_settings
            .chosen
            .insert(flavour.clone(), key.clone());
        self.game_settings.chosen.insert("__last_run".into(), key);
        if let Err(e) = self.game_settings.save() {
            self.line(LineKind::Error, format!("cannot save game settings: {e}"));
        }
        self.line(
            LineKind::Build,
            format!(
                "Run in Game: {} ({})",
                client.label(),
                client.folder.display()
            ),
        );
        self.pending_deploy = Some((flavour, client));
        self.rebuild();
    }

    /// The "which version?" dialog of Run in Game.
    fn ui_game_picker(&mut self, ctx: &egui::Context) {
        let Some(mut selected) = self.game_picker else {
            return;
        };
        let flavours = self.project_flavours();
        // Project targets first, then the rest.
        let mut order: Vec<usize> = (0..self.game_clients.len()).collect();
        order.sort_by_key(|&i| {
            let c = &self.game_clients[i];
            let rank = c
                .flavour
                .as_ref()
                .and_then(|f| flavours.iter().position(|x| x == f))
                .unwrap_or(usize::MAX);
            (rank, c.folder.clone())
        });
        let usable = |c: &crate::game::Client| {
            c.exe.is_some() && c.flavour.as_ref().is_some_and(|f| flavours.contains(f))
        };
        if !self.game_clients.get(selected).is_some_and(usable) {
            selected = order
                .iter()
                .copied()
                .find(|&i| usable(&self.game_clients[i]))
                .unwrap_or(selected);
        }
        let mut result: Option<bool> = None;
        egui::Modal::new(egui::Id::unique("game-picker")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(egui::RichText::new("Run in Game").size(16.0).strong());
            ui.label(
                egui::RichText::new("Which version of World of Warcraft?").color(palette::MUTED),
            );
            ui.add_space(8.0);
            if self.game_clients.is_empty() {
                ui.label(
                    egui::RichText::new("No World of Warcraft install found.")
                        .color(palette::WARNING),
                );
            }
            for &i in &order {
                let c = &self.game_clients[i];
                let ok = usable(c);
                let (r, resp) =
                    ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), Sense::click());
                let on = i == selected;
                let p = ui.painter();
                if on {
                    p.rect_filled(r, 6.0, palette::ACCENT_SOFT);
                    p.rect_stroke(
                        r,
                        6.0,
                        Stroke::new(1.0, palette::ACCENT),
                        StrokeKind::Inside,
                    );
                } else if resp.hovered() && ok {
                    p.rect_filled(r, 6.0, palette::HOVER);
                }
                let title = c.flavour.clone().unwrap_or_else(|| "unknown".into());
                let fg = if ok { palette::TEXT } else { palette::FAINT };
                p.text(
                    r.left_center() + Vec2::new(14.0, -8.0),
                    Align2::LEFT_CENTER,
                    &title,
                    FontId::proportional(14.0),
                    fg,
                );
                let detail = match (&c.exe, ok) {
                    (None, _) => format!("{}  ·  no game executable", c.folder_name()),
                    (Some(_), false) => format!(
                        "{}  ·  not a target of this project (Project Settings)",
                        c.folder_name()
                    ),
                    (Some(e), true) => format!(
                        "{}  ·  {}",
                        c.folder_name(),
                        e.file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                    ),
                };
                p.text(
                    r.left_center() + Vec2::new(14.0, 9.0),
                    Align2::LEFT_CENTER,
                    detail,
                    FontId::proportional(11.5),
                    palette::MUTED,
                );
                if ok && resp.clicked() {
                    selected = i;
                }
                if ok && resp.double_clicked() {
                    selected = i;
                    result = Some(true);
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let can = self.game_clients.get(selected).is_some_and(usable);
                let run = egui::Button::new(egui::RichText::new("Run").color(Color32::WHITE))
                    .fill(palette::ACCENT);
                if ui.add_enabled(can, run).clicked() {
                    result = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    result = Some(false);
                }
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("Game Installs...").frame(false))
                        .clicked()
                    {
                        self.games_open = true;
                        result = Some(false);
                    }
                });
            });
            ui.input(|i| {
                if i.key_pressed(Key::Enter) && result.is_none() {
                    result = Some(true);
                }
                if i.key_pressed(Key::Escape) {
                    result = Some(false);
                }
                // Up/Down move between usable entries.
                let step = if i.key_pressed(Key::ArrowDown) {
                    1i32
                } else if i.key_pressed(Key::ArrowUp) {
                    -1
                } else {
                    0
                };
                if step != 0
                    && let Some(pos) = order.iter().position(|&x| x == selected)
                {
                    let mut k = pos as i32;
                    loop {
                        k += step;
                        if k < 0 || k >= order.len() as i32 {
                            break;
                        }
                        if usable(&self.game_clients[order[k as usize]]) {
                            selected = order[k as usize];
                            break;
                        }
                    }
                }
            });
        });
        match result {
            Some(true) => {
                self.game_picker = None;
                if let Some(c) = self
                    .game_clients
                    .get(selected)
                    .filter(|c| usable(c))
                    .cloned()
                {
                    self.start_in_game(c);
                }
            }
            Some(false) => self.game_picker = None,
            None => self.game_picker = Some(selected),
        }
    }

    /// After a successful build: copy the addon and start the game.
    fn deploy(&mut self, flavour: &str, client: &crate::game::Client, force: bool) {
        let Some(project) = self.opts.project.clone() else {
            return;
        };
        let Some(built) = crate::game::built_folder(&project, flavour, self.opts.target.as_deref())
        else {
            self.line(
                LineKind::Error,
                format!(
                    "no built {flavour} addon under {}",
                    project.join("dist").display()
                ),
            );
            return;
        };
        let checked =
            match selu_manifest::resolution::compiled_manifest_for_target(&built, Some(flavour)) {
                Ok(Some(manifest)) => manifest,
                Ok(None) => {
                    self.line(
                    LineKind::Error,
                    "the built addon has no checked target snapshot; rebuild before Run in Game",
                );
                    return;
                }
                Err(error) => {
                    self.line(LineKind::Error, error.to_string());
                    return;
                }
            };
        if let Some(installed) = crate::game::installed_build(client)
            && installed != checked.build
        {
            self.line(
                LineKind::Build,
                format!(
                    "WARNING: installed {installed} differs from locked {}",
                    checked.target_string()
                ),
            );
        }
        match crate::game::install_addon(&built, client, force) {
            Ok(dest) => {
                self.line(LineKind::Build, format!("copied to {}", dest.display()));
                if crate::game::is_running(client) {
                    self.line(
                        LineKind::Build,
                        "World of Warcraft is already running: type /reload in game to load the new version",
                    );
                } else {
                    match crate::game::launch(client, self.game_settings.launch) {
                        Ok(how) => self.line(LineKind::Build, how),
                        Err(e) => self.line(LineKind::Error, e),
                    }
                }
            }
            Err(crate::game::CopyError::NotOurs(dest)) => {
                self.confirm_replace = Some((flavour.to_string(), client.clone(), dest));
            }
            Err(crate::game::CopyError::Io(e)) => self.line(LineKind::Error, e),
        }
    }

    /// Asked before replacing an addon folder Studio didn't create.
    fn ui_confirm_replace(&mut self, ctx: &egui::Context) {
        let Some((flavour, client, dest)) = self.confirm_replace.clone() else {
            return;
        };
        let mut done = None;
        egui::Modal::new(egui::Id::unique("confirm-replace")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(
                egui::RichText::new("Replace this addon folder?")
                    .size(16.0)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(dest.display().to_string())
                    .monospace()
                    .color(palette::TEXT),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "A folder with this name already exists and Selu Studio didn't create it. \
                     Replacing it moves it to the Recycle Bin.",
                )
                .color(palette::MUTED),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let replace =
                    egui::Button::new(egui::RichText::new("Replace").color(Color32::WHITE))
                        .fill(palette::ERROR.gamma_multiply(0.8));
                if ui.add(replace).clicked() {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) => {
                self.confirm_replace = None;
                self.deploy(&flavour, &client, true);
            }
            Some(false) => {
                self.confirm_replace = None;
                self.line(
                    LineKind::Log,
                    "Run in Game cancelled; the existing folder is unchanged",
                );
            }
            None => {}
        }
    }

    /// Game Installs: the clients found, which one each flavour uses, and
    /// folders added by hand.
    fn ui_games(&mut self, ctx: &egui::Context) {
        if !self.games_open {
            return;
        }
        self.ensure_clients();
        let mut open = true;
        let mut changed = false;
        let previous_ui = (
            self.game_settings.ui_mode,
            self.game_settings.core_ui_addons.clone(),
            self.game_settings.default_ui_source.clone(),
        );
        let mut rescan = false;
        egui::Window::new(egui::RichText::new("Game Installs").size(15.0).strong())
            .open(&mut open)
            .collapsible(false)
            .default_width(720.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Run in Game copies the built addon into the chosen client's Interface\\AddOns and starts it.",
                    )
                    .color(palette::MUTED),
                );
                ui.add_space(8.0);
                if self.game_clients.is_empty() {
                    ui.label(egui::RichText::new("No World of Warcraft install found. Add its folder below.").color(palette::WARNING));
                }
                let flavours = ["wow", "wowf", "wow_classic", "wow_classic_era", "wow_anniversary", "wow_classic_titan", "wowt", "wowxptr", "wow_beta", "wow_classic_ptr", "wow_classic_beta", "wow_classic_era_ptr", "wow_anniversary_ptr"];
                egui::Grid::new("clients").num_columns(5).spacing([14.0, 8.0]).striped(true).show(ui, |ui| {
                    for h in ["USE", "CLIENT", "FLAVOUR", "GAME", "ADDONS FOLDER"] {
                        ui.label(egui::RichText::new(h).size(10.5).strong().color(palette::FAINT));
                    }
                    ui.end_row();
                    for i in 0..self.game_clients.len() {
                        let c = self.game_clients[i].clone();
                        let key = c.folder.to_string_lossy().into_owned();
                        let chosen = c.flavour.as_ref().and_then(|f| {
                            crate::game::pick(&self.game_clients, f, &self.game_settings)
                                .map(|p| p.folder == c.folder)
                        });
                        let mut use_it = chosen.unwrap_or(false);
                        if ui
                            .add_enabled(c.flavour.is_some(), egui::Checkbox::without_text(&mut use_it))
                            .on_hover_text("Run in Game uses this client for its flavour")
                            .changed()
                            && use_it
                            && let Some(f) = &c.flavour
                        {
                            self.game_settings.chosen.insert(f.clone(), key.clone());
                            changed = true;
                        }
                        ui.label(egui::RichText::new(c.folder_name()).monospace());
                        let mut flavour = c.flavour.clone().unwrap_or_default();
                        egui::ComboBox::from_id_salt(("client-flavour", i))
                            .width(170.0)
                            .selected_text(if flavour.is_empty() { "unknown" } else { &flavour })
                            .show_ui(ui, |ui| {
                                for f in flavours {
                                    if ui.selectable_value(&mut flavour, f.to_string(), f).clicked() {
                                        self.game_settings.flavours.insert(key.clone(), f.to_string());
                                        changed = true;
                                        rescan = true;
                                    }
                                }
                            });
                        match &c.exe {
                            Some(e) => ui.label(e.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())),
                            None => ui.label(egui::RichText::new("no game executable").color(palette::WARNING)),
                        };
                        let addons = c.addons_dir();
                        if ui
                            .add(egui::Label::new(egui::RichText::new(addons.display().to_string()).small().color(palette::MUTED)).sense(Sense::click()))
                            .on_hover_text("Open in File Explorer")
                            .clicked()
                        {
                            let _ = std::process::Command::new("explorer").arg(&addons).spawn();
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(10.0);
                crate::theme::rule(ui);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Add install folder");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.game_new_root)
                            .hint_text(r"C:\Program Files (x86)\World of Warcraft")
                            .desired_width(360.0),
                    );
                    if ui.button("Add").clicked() {
                        let p = std::path::PathBuf::from(self.game_new_root.trim());
                        if p.is_dir() {
                            self.game_settings.extra_roots.push(p);
                            self.game_new_root.clear();
                            changed = true;
                            rescan = true;
                        } else {
                            self.line(LineKind::Error, format!("{} is not a folder", p.display()));
                        }
                    }
                    if ui.button("Rescan").clicked() {
                        rescan = true;
                    }
                });
                ui.add_space(6.0);
                ui.label("Client assets (takes effect after restarting Studio)");
                for mode in [crate::game::UiMode::None, crate::game::UiMode::Core, crate::game::UiMode::Full] {
                    changed |= ui.radio_value(&mut self.game_settings.ui_mode,mode,format!("{mode:?}")).changed();
                }
                let mut roots=self.game_settings.core_ui_addons.join(", ");
                if ui.text_edit_singleline(&mut roots).changed() {
                    self.game_settings.core_ui_addons=roots.split([',',' ']).filter(|s|!s.is_empty()).map(str::to_owned).collect();
                    changed=true;
                }
                ui.label("Blizzard UI source checkout (optional; the installed client is used first, and a checkout only when its branch matches the product)");
                let mut source=self.game_settings.default_ui_source.as_ref().map(|p|p.display().to_string()).unwrap_or_default();
                if ui.text_edit_singleline(&mut source).changed() {
                    self.game_settings.default_ui_source = if source.trim().is_empty() { None } else { Some(PathBuf::from(source.trim())) };
                    changed=true;
                }
                changed |= ui.checkbox(&mut self.game_settings.disable_client_assets, "Disable client fonts and art").changed();
                let mut root = self.game_settings.client_install.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                if ui.text_edit_singleline(&mut root).changed() {
                    self.game_settings.client_install = if root.trim().is_empty() { None } else { Some(PathBuf::from(root.trim())) };
                    changed = true;
                }
                ui.label("Install root containing .build.info; leave empty to detect automatically.");
                ui.label(egui::RichText::new("Start the game").strong());
                for mode in [
                    crate::game::LaunchMode::Battlenet,
                    crate::game::LaunchMode::BattlenetPage,
                    crate::game::LaunchMode::Direct,
                ] {
                    if ui
                        .radio_value(&mut self.game_settings.launch, mode, mode.label())
                        .changed()
                    {
                        changed = true;
                    }
                }
                if self.game_settings.launch != crate::game::LaunchMode::Direct
                    && crate::game::battlenet_exe().is_none()
                {
                    ui.label(
                        egui::RichText::new("Battle.net wasn't found; the game will be started directly.")
                            .small()
                            .color(palette::WARNING),
                    );
                }
                if let Some(f) = self.game_flavour() {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(format!("This project runs on {f}.")).small().color(palette::FAINT));
                }
            });
        if changed && let Err(e) = self.game_settings.save() {
            self.line(LineKind::Error, format!("cannot save game settings: {e}"));
        }
        if previous_ui
            != (
                self.game_settings.ui_mode,
                self.game_settings.core_ui_addons.clone(),
                self.game_settings.default_ui_source.clone(),
            )
        {
            self.restart_session();
        }
        if rescan {
            self.game_scanned = false;
            self.ensure_clients();
        }
        if !open {
            self.games_open = false;
        }
    }

    /// File > Open Project: pick a folder that contains `selu.toml`.
    fn open_project_dialog(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().set_title("Open Selu project");
        if let Some(dir) = self.opts.project.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(dir) = dialog.pick_folder() {
            self.open_project(dir, ctx);
        }
    }

    /// Open another project in this window: the editor switches at once, a
    /// fresh simulator session starts, and the project builds in the
    /// background. Build errors show in Problems; the project stays open.
    fn open_project(&mut self, picked: PathBuf, ctx: &egui::Context) {
        let Some(dir) = find_project(&picked) else {
            let message = format!(
                "{} contains neither a plain addon TOC nor a Selu project.",
                picked.display()
            );
            self.line(LineKind::Error, message.clone());
            self.open_status = Some(OpenStatus::Failed(message));
            return;
        };
        if self.editor.any_dirty() {
            self.open_status = Some(OpenStatus::Failed(
                "Save your changes first (File > Save All), then open the other project.".into(),
            ));
            return;
        }
        let selu = self.opts.selu.clone().unwrap_or_else(|| {
            // Studio started directly: use the selu next to selu-studio, else PATH.
            std::env::current_exe()
                .ok()
                .and_then(|e| {
                    e.parent()
                        .map(|p| p.join(if cfg!(windows) { "selu.exe" } else { "selu" }))
                })
                .filter(|p| p.is_file())
                .unwrap_or_else(|| PathBuf::from("selu"))
        });
        // The project's own targets and lock decide what to build and load.
        self.saved_seed = None;
        self.saved_identity = None;
        self.saved_choice = vec![String::new(); 3];
        self.saved_lists = Default::default();
        self.saved_scanned = None;
        self.saved_status.clear();
        self.opts.project = Some(dir.clone());
        self.opts.selu = Some(selu);
        let plain = plain_addon(&dir);
        let manifests = self
            .opts
            .manifest_dir
            .clone()
            .or_else(|| crate::editor::complete::find_manifests(&dir));
        self.opts.manifest = if plain {
            manifests
                .as_deref()
                .and_then(|m| addon_manifest(&dir, m, self.opts.target.as_deref()))
        } else {
            None
        };
        self.opts.target = self
            .opts
            .manifest
            .as_ref()
            .map(selu_manifest::Manifest::target_string);
        self.opts.addons.clear();
        self.editor = crate::editor::Editor::new(
            Some(dir.clone()),
            if plain { None } else { self.opts.selu.clone() },
            self.opts.target.clone(),
            ctx,
        );
        if let Some(m) = &self.opts.manifest {
            self.editor.set_api(m);
        }
        if plain {
            self.opts.addons = vec![dir.clone()];
        }
        self.sources_stamp = sources_stamp(&dir);
        self.auto_rebuild = true;
        self.side = Side::Explorer;
        self.tab = Tab::Problems;
        self.selected = None;
        self.pending_deploy = None;
        // A build of the previous project may still run; its result is ignored.
        self.build = None;
        self.restart_session();
        let recent = &mut self.game_settings.recent_projects;
        recent.retain(|p| p != &dir);
        recent.insert(0, dir.clone());
        recent.truncate(10);
        if let Err(e) = self.game_settings.save() {
            self.line(LineKind::Error, format!("cannot save recent projects: {e}"));
        }
        self.line(LineKind::Build, format!("opened {}", dir.display()));
        self.load_after_build = true;
        self.playing = false;
        if !plain {
            self.rebuild();
        }
    }

    /// Whether there is an addon to simulate. A project opens before its
    /// first successful build; until then the game clock, reload and the
    /// time buttons are off, and the preview says why.
    fn sim_ready(&self) -> bool {
        !self.opts.addons.is_empty()
    }

    /// Why the simulator is not running yet, for the preview.
    fn not_ready_reason(&self) -> (bool, String) {
        let name = self
            .opts
            .project
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(
                || "the project".to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
        if self.build.is_some() {
            (
                true,
                format!("Building {name}...\nThe game starts when the build is done."),
            )
        } else if self.build_failed {
            (
                false,
                "The project does not build yet.\nFix the errors in Problems and save; the game starts after the first successful build.".into(),
            )
        } else if self.opts.project.is_some() {
            (
                false,
                "Nothing to run yet: no built addon was found.\nBuild the project (Ctrl+B).".into(),
            )
        } else {
            (false, "No addon loaded.".into())
        }
    }

    /// A fresh simulator session for the current options: client assets,
    /// the built addon folders (if any), login and a first snapshot.
    fn restart_session(&mut self) {
        let mut assets = selu_sim::assets::Resolver::default();
        if let Some(root) = &self.opts.client_install {
            let target = self
                .opts
                .manifest
                .as_ref()
                .map(selu_manifest::Manifest::target_string)
                .or_else(|| self.opts.target.clone());
            if let Some(target) = target {
                let _ = assets.open_client(root, target.split('@').next().unwrap_or_default());
            }
        }
        for root in &self.opts.addons {
            assets.add_addon(root.clone());
        }
        self.assets = assets;
        self.textures.clear();
        self.worker = Worker::spawn(SessionConfig {
            client_install: self.opts.client_install.clone(),
            manifest: self.opts.manifest.clone(),
            target: self.opts.target.clone(),
            character: self
                .saved_identity
                .as_ref()
                .map(|v| v.0.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| SessionConfig::default().character),
            realm: self
                .saved_identity
                .as_ref()
                .map(|v| v.1.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| SessionConfig::default().realm),
            trace: true,
            ..SessionConfig::default()
        });
        self.in_flight = 0;
        self.snapshot = None;
        if let Some((dir, values)) = self.saved_seed.clone() {
            self.send(Command::SeedSaved { dir, values });
        }
        self.queue_default_ui();
        for dir in self.opts.addons.clone() {
            self.send(Command::LoadAddon { dir });
        }
        self.send(Command::Login);
        self.send(Command::Snapshot);
    }

    fn queue_default_ui(&mut self) {
        if self.game_settings.ui_mode == crate::game::UiMode::None {
            return;
        }
        let target = self
            .opts
            .manifest
            .as_ref()
            .map(selu_manifest::Manifest::target_string)
            .or_else(|| self.opts.target.clone());
        let Some(target) = target else {
            return;
        };
        let (source, skipped) = crate::game::ui_source(
            self.game_settings.default_ui_source.as_deref(),
            self.opts.client_install.as_deref(),
            &target,
        );
        if let Some(note) = skipped {
            self.line(LineKind::Error, note);
        }
        if let Some(source) = source {
            if self.game_settings.ui_mode == crate::game::UiMode::Core {
                self.send(Command::LoadCoreUi {
                    source,
                    roots: self.game_settings.core_ui_addons.clone(),
                });
            } else {
                self.send(Command::LoadDefaultUi { source });
            }
        }
    }

    /// After the first successful build of a newly opened project: load the
    /// built addon (and its checked manifest) into a fresh session.
    fn load_built_project(&mut self) {
        let Some(project) = self.opts.project.clone() else {
            return;
        };
        let Some(addon) = built_addon(&project) else {
            self.line(
                LineKind::Error,
                format!(
                    "built, but no addon folder found in {}",
                    project.join("dist").display()
                ),
            );
            return;
        };
        let snapshot = selu_manifest::resolution::compiled_manifest_path(&addon);
        self.opts.manifest = selu_manifest::resolution::load_checked_manifest(&snapshot, None).ok();
        self.opts.addons = vec![addon.clone()];
        self.line(LineKind::Build, format!("loading {}", addon.display()));
        self.restart_session();
    }

    /// The Open Project dialog: shown while the project builds, and on failure.
    fn ui_open_status(&mut self, ctx: &egui::Context) {
        let Some(status) = &self.open_status else {
            return;
        };
        let mut close = false;
        egui::Modal::new(egui::Id::unique("open-project")).show(ctx, |ui| {
            ui.set_width(520.0);
            match status {
                OpenStatus::Failed(message) => {
                    ui.label(
                        egui::RichText::new("Cannot open project")
                            .size(16.0)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(message).monospace());
                        });
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            }
        });
        if close {
            self.open_status = None;
        }
    }

    fn global_shortcuts(&mut self, ctx: &egui::Context) {
        self.editor.shortcuts(ctx);
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Comma)) {
            self.open_settings();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::O)) {
            self.open_project_dialog(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F5)) {
            self.run_in_game();
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
        if run && self.sim_ready() {
            self.playing = !self.playing;
        }
        if reload && self.sim_ready() {
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
                            ("Ctrl+F5", "Run in Game: build, copy and start WoW"),
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

    /// Resize from the window edges (the window has no system frame).
    fn edge_resize(&self, ctx: &egui::Context) {
        use egui::{CursorIcon, ResizeDirection as D};
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let screen = ctx.content_rect();
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let m = 5.0;
        let (w, e) = (pos.x - screen.left() < m, screen.right() - pos.x < m);
        let (n, s) = (pos.y - screen.top() < m, screen.bottom() - pos.y < m);
        let dir = match (n, s, w, e) {
            (true, _, true, _) => D::NorthWest,
            (true, _, _, true) => D::NorthEast,
            (_, true, true, _) => D::SouthWest,
            (_, true, _, true) => D::SouthEast,
            (true, ..) => D::North,
            (_, true, ..) => D::South,
            (_, _, true, _) => D::West,
            (_, _, _, true) => D::East,
            _ => return,
        };
        ctx.set_cursor_icon(match dir {
            D::North | D::South => CursorIcon::ResizeVertical,
            D::East | D::West => CursorIcon::ResizeHorizontal,
            D::NorthWest | D::SouthEast => CursorIcon::ResizeNwSe,
            D::NorthEast | D::SouthWest => CursorIcon::ResizeNeSw,
        });
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }

    /// Minimize, maximize/restore and close, drawn like Windows 11's.
    fn window_buttons(&self, ui: &mut egui::Ui, bar: egui::Rect) {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        let w = 46.0;
        let buttons = [
            (icon::WIN_MINIMIZE, "Minimize", 0),
            (
                if maximized {
                    icon::WIN_RESTORE
                } else {
                    icon::WIN_MAXIMIZE
                },
                if maximized { "Restore" } else { "Maximize" },
                1,
            ),
            (icon::WIN_CLOSE, "Close", 2),
        ];
        for (k, (glyph, tip, action)) in buttons.into_iter().enumerate() {
            let right = bar.right() - (2 - k) as f32 * w;
            let r = egui::Rect::from_min_max(
                egui::pos2(right - w, bar.top()),
                egui::pos2(right, bar.bottom()),
            );
            let resp = ui.interact(r, egui::Id::unique(("window-button", k)), Sense::click());
            let (fill, fg) = match (action, resp.hovered(), resp.is_pointer_button_down_on()) {
                (2, true, _) => (Color32::from_rgb(0xc4, 0x2b, 0x1c), Color32::WHITE),
                (_, _, true) => (palette::RAISED, Color32::WHITE),
                (_, true, _) => (palette::HOVER, Color32::WHITE),
                _ => (Color32::TRANSPARENT, palette::TEXT),
            };
            ui.painter().rect_filled(r, 0.0, fill);
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                glyph,
                icon_font(10.0),
                fg,
            );
            if resp.on_hover_text(tip).clicked() {
                let cmd = match action {
                    0 => egui::ViewportCommand::Minimized(true),
                    1 => egui::ViewportCommand::Maximized(!maximized),
                    _ => egui::ViewportCommand::Close,
                };
                ui.ctx().send_viewport_cmd(cmd);
            }
        }
    }

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        // The bar itself moves the window and maximizes on double-click.
        // Widgets added after this take precedence over it.
        let bar = ui.interact(rect, egui::Id::unique("title-bar"), Sense::click_and_drag());
        if bar.double_clicked() {
            let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        } else if bar.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        ui.horizontal_centered(|ui| {
            match &self.logo {
                Some(t) => {
                    ui.add(egui::Image::new(t).fit_to_exact_size(egui::vec2(20.0, 20.0)));
                }
                None => {
                    ui.label(icon_text(icon::CODE, 15.0).color(palette::ACCENT));
                }
            }
            ui.add_space(2.0);
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add(egui::Button::new("Open Project...").shortcut_text("Ctrl+O"))
                        .clicked()
                    {
                        ui.close();
                        self.open_project_dialog(ui.ctx());
                    }
                    let recent: Vec<PathBuf> = self
                        .game_settings
                        .recent_projects
                        .iter()
                        .filter(|p| Some(*p) != self.opts.project.as_ref())
                        .cloned()
                        .collect();
                    ui.add_enabled_ui(!recent.is_empty(), |ui| {
                        ui.menu_button("Open Recent", |ui| {
                            for dir in recent {
                                let name = dir.file_name().map_or_else(
                                    || dir.display().to_string(),
                                    |n| n.to_string_lossy().into_owned(),
                                );
                                if ui
                                    .button(name)
                                    .on_hover_text(dir.display().to_string())
                                    .clicked()
                                {
                                    ui.close();
                                    self.open_project(dir, ui.ctx());
                                }
                            }
                        });
                    });
                    ui.separator();
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
                ui.menu_button("Build", |ui| {
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
                    if ui
                        .add(egui::Button::new("Format Document").shortcut_text("Shift+Alt+F"))
                        .clicked()
                    {
                        let now = ui.input(|i| i.time);
                        self.editor.format_active(now);
                    }
                });
                ui.menu_button("Run", |ui| {
                    if ui
                        .add_enabled(
                            self.editor.has_project(),
                            egui::Button::new("Run in Game").shortcut_text("Ctrl+F5"),
                        )
                        .on_hover_text(
                            "Build, copy into the game's AddOns folder and start the game",
                        )
                        .clicked()
                    {
                        self.run_in_game();
                    }
                    if ui.button("Game Installs...").clicked() {
                        self.games_open = true;
                    }
                    ui.separator();
                    let ready = self.sim_ready();
                    let label = if self.playing { "Pause" } else { "Run" };
                    if ui
                        .add_enabled(ready, egui::Button::new(label).shortcut_text("F5"))
                        .clicked()
                    {
                        self.playing = !self.playing;
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("/reload").shortcut_text("Ctrl+R"))
                        .clicked()
                    {
                        self.act(Command::Reload);
                    }
                    ui.separator();
                    for (label, seconds) in [
                        ("Advance 0.1 s", 0.1),
                        ("Advance 1 s", 1.0),
                        ("Advance 10 s", 10.0),
                    ] {
                        if ui.add_enabled(ready, egui::Button::new(label)).clicked() {
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
        let ready = self.sim_ready();
        let (icon, tip) = if !ready {
            (icon::PLAY, "Nothing to run until the project builds")
        } else if self.playing {
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
        if child
            .add_enabled(ready, play)
            .on_hover_text(tip)
            .on_disabled_hover_text(tip)
            .clicked()
        {
            self.playing = !self.playing;
        }
        if child
            .add_enabled(
                ready,
                egui::Button::new(icon_text(icon::REFRESH, 13.0)).min_size(egui::vec2(32.0, 26.0)),
            )
            .on_hover_text("/reload (Ctrl+R)")
            .clicked()
        {
            self.act(Command::Reload);
        }
        let can_build = self.opts.project.is_some() && self.opts.selu.is_some();
        if self.build.is_some() {
            let (r, _) = child.allocate_exact_size(egui::vec2(32.0, 26.0), Sense::hover());
            egui::Spinner::new()
                .size(14.0)
                .color(palette::ACCENT)
                .paint_at(&child, r.shrink(6.0));
        } else {
            let color = if self.build_failed {
                palette::ERROR
            } else {
                palette::TEXT
            };
            let tip = if self.build_failed {
                "The last build failed; see the console (Ctrl+B builds again)"
            } else {
                "Build (Ctrl+B)"
            };
            if child
                .add_enabled(
                    can_build,
                    egui::Button::new(icon_text(icon::BUILD, 13.0).color(color))
                        .min_size(egui::vec2(32.0, 26.0)),
                )
                .on_hover_text(tip)
                .clicked()
            {
                self.rebuild();
            }
        }
        if child
            .add_enabled(
                can_build,
                egui::Button::new(icon_text(icon::GAME, 13.0).color(palette::ACCENT))
                    .min_size(egui::vec2(32.0, 26.0)),
            )
            .on_hover_text("Run in Game (Ctrl+F5): build, copy into the game's AddOns and start it")
            .clicked()
        {
            self.run_in_game();
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
        // Window buttons, and the project name left of them.
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() - 8.0, rect.top()),
            egui::pos2(rect.right() + 8.0, rect.bottom()),
        );
        self.window_buttons(ui, bar_rect);
        let title = format!("{} — Selu Studio", self.editor.project_name());
        ui.painter().text(
            egui::pos2(bar_rect.right() - 3.0 * 46.0 - 14.0, rect.center().y),
            Align2::RIGHT_CENTER,
            title,
            FontId::proportional(12.5),
            palette::FAINT,
        );
    }

    fn wtf_root(&self) -> Option<PathBuf> {
        if !self.game_settings.wtf_override.trim().is_empty() {
            return Some(PathBuf::from(self.game_settings.wtf_override.trim()));
        }
        let root = self.opts.client_install.as_ref()?;
        if root
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(crate::game::flavour_of)
            .is_some()
        {
            return Some(root.join("WTF"));
        }
        let target = self
            .opts
            .manifest
            .as_ref()
            .map(selu_manifest::Manifest::target_string)
            .or_else(|| self.opts.target.clone())?;
        let product = target.split('@').next()?;
        Some(crate::game::product_folder(root, product)?.join("WTF"))
    }
    fn saved_ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Saved variables", |ui| {
            ui.label("WTF folder override (blank uses configured client)");
            if ui
                .text_edit_singleline(&mut self.game_settings.wtf_override)
                .lost_focus()
            {
                let _ = self.game_settings.save();
            }
            if ui.button("Clear").clicked() {
                self.saved_seed = None;
                self.saved_identity = None;
                self.saved_status = "Empty saved data".into();
                self.restart_session();
            }
            let Some(root) = self.wtf_root() else {
                ui.label("Configure a client install or WTF override.");
                return;
            };
            ui.label(root.display().to_string());
            let project = self
                .opts
                .project
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let rescan = ui.button("Rescan accounts").clicked();
            if rescan || self.saved_scanned.as_ref() != Some(&root) {
                self.saved_scanned = Some(root.clone());
                if let Some(choice) = self
                    .game_settings
                    .saved_choices
                    .get(&project)
                    .filter(|v| v.len() == 3)
                {
                    self.saved_choice = choice.clone();
                }
                let accounts = root.join("Account");
                self.saved_lists = Default::default();
                self.saved_scan_note.clear();
                match saved_dirs(&accounts) {
                    Ok(list) if list.is_empty() => {
                        self.saved_scan_note = format!(
                            "No accounts in {}. Log in once with this client so WoW creates it.",
                            accounts.display()
                        );
                    }
                    Ok(list) => self.saved_lists[0] = list,
                    Err(e) => self.saved_scan_note = e,
                }
                // A remembered choice that no longer exists is dropped.
                for i in 0..3 {
                    let known = self.saved_lists[i].contains(&self.saved_choice[i]);
                    if !known {
                        for j in i..3 {
                            self.saved_choice[j].clear();
                        }
                        break;
                    }
                    if i < 2 {
                        let mut path = accounts.clone();
                        for c in &self.saved_choice[..=i] {
                            path.push(c);
                        }
                        self.saved_lists[i + 1] = saved_dirs(&path).unwrap_or_default();
                    }
                }
            }
            for (i, label) in ["Account", "Realm", "Character"].iter().enumerate() {
                let before = self.saved_choice[i].clone();
                egui::ComboBox::from_id_salt(label)
                    .selected_text(format!("{label}: {}", self.saved_choice[i]))
                    .show_ui(ui, |ui| {
                        for name in &self.saved_lists[i] {
                            ui.selectable_value(&mut self.saved_choice[i], name.clone(), name);
                        }
                    });
                if before != self.saved_choice[i] && i < 2 {
                    for j in i + 1..3 {
                        self.saved_choice[j].clear();
                        self.saved_lists[j].clear();
                    }
                    let mut path = root.join("Account");
                    for j in 0..=i {
                        path.push(&self.saved_choice[j]);
                    }
                    self.saved_lists[i + 1] = saved_dirs(&path).unwrap_or_default();
                }
            }
            if !self.saved_scan_note.is_empty() {
                ui.label(egui::RichText::new(&self.saved_scan_note).color(palette::MUTED));
            }
            if ui
                .add_enabled(!self.saved_choice[0].is_empty(), egui::Button::new("Load"))
                .clicked()
            {
                if let Some(dir) = self.opts.addons.first().cloned() {
                    match selu_sim::loader::read_toc_target(&dir, self.opts.target.as_deref()) {
                        Ok(toc) => {
                            let result = selu_sim::wtf::load(
                                &root,
                                &self.saved_choice[0],
                                &self.saved_choice[1],
                                &self.saved_choice[2],
                                &toc,
                            );
                            self.saved_status = format!(
                                "Loaded: {}\n{}",
                                result.values.keys().cloned().collect::<Vec<_>>().join(", "),
                                result.warnings.join("\n")
                            );
                            self.saved_seed = Some((dir, result.values));
                            self.saved_identity =
                                Some((self.saved_choice[2].clone(), self.saved_choice[1].clone()));
                            self.game_settings
                                .saved_choices
                                .insert(project, self.saved_choice.clone());
                            let _ = self.game_settings.save();
                            self.restart_session();
                        }
                        Err(e) => self.saved_status = e,
                    }
                } else {
                    self.saved_status = "Build the open project first.".into();
                }
            }
            ui.label(&self.saved_status);
        });
    }

    fn scenario_ui(&mut self, ui: &mut egui::Ui) {
        self.saved_ui(ui);
        caption(ui, "Scenario");
        let mut apply = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label("Quick actions");
            if ui.button("Target a rare elite").clicked() {
                let u = selu_sim::world::Unit {
                    classification: "rareelite".into(),
                    ..Default::default()
                };
                self.scenario_unit = u.clone();
                self.scenario.units.insert("target".into(), u);
                apply = true;
            }
            if ui.button("Rare spawns nearby").clicked() {
                let u = selu_sim::world::Unit {
                    classification: "rare".into(),
                    ..Default::default()
                };
                self.scenario.units.insert("nameplate1".into(), u.clone());
                self.scenario
                    .vignettes
                    .retain(|v| v.guid != "Vignette-scenario-1");
                self.scenario.vignettes.push(selu_sim::world::Vignette {
                    guid: "Vignette-scenario-1".into(),
                    object_guid: u.guid,
                    name: u.name,
                    map: self.scenario.map,
                    x: 0.5,
                    y: 0.5,
                });
                apply = true;
            }
            if ui
                .button(if self.scenario.combat {
                    "Leave combat"
                } else {
                    "Enter combat"
                })
                .clicked()
            {
                self.scenario.combat = !self.scenario.combat;
                apply = true;
            }
            if ui.button("Target dies").clicked()
                && let Some(u) = self.scenario.units.get_mut("target")
            {
                u.dead = true;
                u.health = 0;
                apply = true;
            }
            ui.separator();
            ui.label("Units");
            egui::ComboBox::from_id_salt("scenario-token")
                .selected_text(&self.scenario_token)
                .show_ui(ui, |ui| {
                    for t in [
                        "player",
                        "target",
                        "focus",
                        "mouseover",
                        "nameplate1",
                        "nameplate2",
                        "party1",
                        "party2",
                        "party3",
                        "party4",
                    ] {
                        if ui
                            .selectable_value(&mut self.scenario_token, t.into(), t)
                            .changed()
                        {
                            self.scenario_unit =
                                self.scenario.units.get(t).cloned().unwrap_or_default();
                        }
                    }
                });
            ui.label("Token (nameplateN supported)");
            ui.text_edit_singleline(&mut self.scenario_token);
            let u = &mut self.scenario_unit;
            ui.label("Name");
            ui.text_edit_singleline(&mut u.name);
            ui.label("GUID");
            ui.text_edit_singleline(&mut u.guid);
            ui.horizontal(|ui| {
                ui.label("Level");
                ui.add(egui::DragValue::new(&mut u.level));
            });
            ui.label("Classification");
            egui::ComboBox::from_id_salt("classification")
                .selected_text(&u.classification)
                .show_ui(ui, |ui| {
                    for c in ["normal", "elite", "rare", "rareelite", "worldboss", "minus"] {
                        ui.selectable_value(&mut u.classification, c.into(), c);
                    }
                });
            ui.label("Creature type");
            ui.text_edit_singleline(&mut u.creature_type);
            ui.horizontal(|ui| {
                ui.label("Reaction");
                ui.add(egui::DragValue::new(&mut u.reaction).range(1..=8));
            });
            ui.horizontal(|ui| {
                ui.label("Health / max");
                ui.add(egui::DragValue::new(&mut u.health));
                ui.add(egui::DragValue::new(&mut u.max_health));
            });
            ui.checkbox(&mut u.dead, "Dead");
            ui.horizontal(|ui| {
                if ui.button("Add / update unit").clicked() {
                    self.scenario
                        .units
                        .insert(self.scenario_token.clone(), self.scenario_unit.clone());
                    apply = true;
                }
                if ui.button("Clear").clicked() {
                    self.scenario.units.remove(&self.scenario_token);
                    apply = true;
                }
            });
            ui.separator();
            ui.label("Vignettes");
            if self.scenario.vignettes.is_empty() {
                ui.label("No vignettes in this scenario.");
            }
            let mut remove = None;
            for (i, v) in self.scenario.vignettes.iter().enumerate() {
                ui.push_id(&v.guid, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(&v.name).on_hover_text(&v.guid);
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                self.scenario.vignettes.remove(i);
                apply = true;
            }
            if ui.button("Add rare here").clicked() {
                let mut n = 1;
                while self
                    .scenario
                    .vignettes
                    .iter()
                    .any(|v| v.guid == format!("Vignette-manual-{n}"))
                {
                    n += 1;
                }
                self.scenario.vignettes.push(selu_sim::world::Vignette {
                    guid: format!("Vignette-manual-{n}"),
                    object_guid: self.scenario_unit.guid.clone(),
                    name: self.scenario_unit.name.clone(),
                    map: self.scenario.map,
                    x: 0.5,
                    y: 0.5,
                });
                apply = true;
            }
            ui.separator();
            apply |= ui
                .checkbox(&mut self.scenario.combat, "In combat")
                .changed();
            ui.horizontal(|ui| {
                ui.label("Money (copper)");
                apply |= ui
                    .add(egui::DragValue::new(&mut self.scenario.money))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Bag ID");
                ui.add(egui::DragValue::new(&mut self.scenario_bag_id).range(0..=4));
            });
            ui.label("Item IDs (comma separated)");
            ui.text_edit_singleline(&mut self.scenario_bag);
            if ui.button("Set bag").clicked() {
                match self
                    .scenario_bag
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().parse::<u32>())
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(items) => {
                        self.scenario.bags.insert(self.scenario_bag_id, items);
                        apply = true;
                    }
                    Err(e) => {
                        ui.label(e.to_string());
                    }
                }
            }
            ui.label("Zone / subzone");
            apply |= ui.text_edit_singleline(&mut self.scenario.zone).changed();
            apply |= ui
                .text_edit_singleline(&mut self.scenario.subzone)
                .changed();
            ui.horizontal(|ui| {
                ui.label("Map ID");
                apply |= ui
                    .add(egui::DragValue::new(&mut self.scenario.map))
                    .changed();
            });
            ui.separator();
            ui.label("Saved scenario");
            ui.text_edit_singleline(&mut self.scenario_name);
            let valid = !self.scenario_name.is_empty()
                && self
                    .scenario_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if let Some(root) = self.editor.project_dir() {
                let path = root
                    .join("scenarios")
                    .join(format!("{}.toml", self.scenario_name));
                ui.add_enabled_ui(valid, |ui| {
                    if ui.button("Save scenario").clicked() {
                        self.scenario_status = match self.scenario.save(&path) {
                            Ok(()) => format!("Saved {}", path.display()),
                            Err(e) => e,
                        };
                    }
                    if ui.button("Load scenario").clicked() {
                        match selu_sim::world::Scenario::load(&path) {
                            Ok(state) => {
                                self.scenario = state;
                                apply = true;
                            }
                            Err(e) => {
                                self.scenario_status = e;
                            }
                        }
                    }
                });
            } else {
                ui.label("Open a project to save scenarios.");
            }
            ui.label(&self.scenario_status);
        });
        if apply {
            self.act(Command::Scenario {
                state: self.scenario.clone(),
            });
        }
    }

    fn activity_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            for (side, icon, tip) in [
                (Side::Explorer, icon::FILES, "Explorer (Ctrl+Shift+E)"),
                (Side::Frames, icon::FRAMES, "Frames"),
                (Side::Scenario, icon::GAME, "Scenario"),
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
            if self.build.is_some() {
                ui.label(small("Building...".into()));
            } else if self.build_failed {
                ui.label(small("Build failed".into()));
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(small(format!("session {}", self.generation)));
                if let Some(s) = self.snapshot.as_ref().filter(|_| self.sim_ready()) {
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
                let ready = self.sim_ready();
                for (label, seconds) in [("+10s", 10.0), ("+1s", 1.0)] {
                    if ui
                        .add_enabled(ready, egui::Button::new(label).small())
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
        if self.sim_ready() {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            self.screen(&mut child);
        } else {
            ui.painter().rect_filled(rect, 4.0, palette::PANEL);
            let (busy, text) = self.not_ready_reason();
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect.shrink(24.0))
                    .layout(egui::Layout::top_down(Align::Center)),
            );
            child.add_space((rect.height() / 2.0 - 40.0).max(0.0));
            if busy {
                child.add(egui::Spinner::new().size(20.0).color(palette::ACCENT));
                child.add_space(8.0);
            }
            let color = if !busy && self.build_failed {
                palette::ERROR
            } else {
                palette::MUTED
            };
            child.label(egui::RichText::new(text).color(color));
        }
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
                            TextureFill::Gradient { .. } => "gradient".into(),
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
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
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
        // Mask regions are inputs to texture compositing, never artwork.
        let visible =
            |r: &RegionSnapshot| r.widget != "MaskTexture" && (r.visible || self.show_hidden);
        let fade = |r: &RegionSnapshot, c: Color32| {
            let a = r.alpha.clamp(0.0, 1.0) as f32 * if r.visible { 1.0 } else { 0.3 };
            c.gamma_multiply(a)
        };
        if self.glyphs.len() > 8192 {
            self.glyphs.clear();
        }
        let mut ordered: Vec<_> = snap.regions.iter().skip(1).filter(|r| visible(r)).collect();
        ordered.sort_by_key(|r| (r.strata, r.level, r.layer, r.id.index));
        for r in &ordered {
            let painter = painter.with_clip_rect(
                r.clip
                    .as_ref()
                    .map(to_screen)
                    .unwrap_or(area)
                    .intersect(area),
            );
            let Some(rect) = r.rect.as_ref().and_then(|rect| paint_rect(to_screen(rect))) else {
                continue;
            };
            if r.widget == "Minimap" {
                let center = rect.center();
                let radius = rect.width().min(rect.height()) / 2.0;
                painter.circle_filled(center, radius, fade(r, Color32::from_rgb(68, 88, 69)));
                for offset in [-0.5f32, 0.0, 0.5] {
                    let y = offset * radius;
                    let extent = (radius * radius - y * y).sqrt();
                    painter.line_segment(
                        [
                            center + Vec2::new(-extent, y),
                            center + Vec2::new(extent, y),
                        ],
                        Stroke::new(1.0, fade(r, Color32::from_rgb(84, 103, 81))),
                    );
                }
            }
            match (&r.texture, r.widget.as_str()) {
                (Some(TextureFill::Gradient { vertical, min, max }), _) => {
                    let color = |c: &[f32; 4]| {
                        let c = crate::textures::desaturated_color(*c, r.visual.desaturated);
                        fade(
                            r,
                            Color32::from_rgba_unmultiplied(
                                (c[0] * r.visual.tint[0] * 255.0) as u8,
                                (c[1] * r.visual.tint[1] * 255.0) as u8,
                                (c[2] * r.visual.tint[2] * 255.0) as u8,
                                (c[3] * r.visual.tint[3] * 255.0) as u8,
                            ),
                        )
                    };
                    let (lo, hi) = (color(min), color(max));
                    let colors = if *vertical {
                        [hi, hi, lo, lo]
                    } else {
                        [lo, hi, hi, lo]
                    };
                    let mut mesh = egui::Mesh::default();
                    for (pos, color) in [
                        rect.left_top(),
                        rect.right_top(),
                        rect.right_bottom(),
                        rect.left_bottom(),
                    ]
                    .into_iter()
                    .zip(colors)
                    {
                        mesh.colored_vertex(pos, color);
                    }
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    painter.add(egui::Shape::mesh(mesh));
                }
                (Some(TextureFill::Color(c)), _) => {
                    let c = crate::textures::desaturated_color(*c, r.visual.desaturated);
                    let tint = r.visual.tint;
                    let col = Color32::from_rgba_unmultiplied(
                        (c[0] * tint[0] * 255.0) as u8,
                        (c[1] * tint[1] * 255.0) as u8,
                        (c[2] * tint[2] * 255.0) as u8,
                        (c[3] * tint[3] * 255.0) as u8,
                    );
                    painter.rect_filled(rect, 0.0, fade(r, col));
                }
                (Some(TextureFill::File(name) | TextureFill::Atlas(name)), _) => {
                    let key = if r.visual.desaturated {
                        format!("\0desaturated:{name}")
                    } else {
                        name.clone()
                    };
                    let cached = self.textures.entry(key.clone()).or_insert_with(|| {
                        if matches!(r.texture, Some(TextureFill::Atlas(_))) {
                            return Err("unavailable atlas".into());
                        }
                        let resolved = crate::textures::load_resolved(&self.assets, name);
                        #[cfg(debug_assertions)]
                        if std::env::var_os("SELU_SIM_PROFILE").is_some()
                            && let Err(error) = &resolved
                        {
                            eprintln!("PROFILE_ASSET {name}: {error}");
                        }
                        resolved.map(|mut image| {
                            if r.visual.desaturated {
                                crate::textures::desaturate(&mut image);
                            }
                            ui.ctx()
                                .load_texture(&key, image, egui::TextureOptions::LINEAR)
                        })
                    });
                    if let Ok(texture) = cached {
                        let mut uv = r.visual.uv;
                        let dimensions = texture.size();
                        for axis in 0..2 {
                            if r.visual.tile[axis] {
                                uv[axis * 2 + 1] = uv[axis * 2]
                                    + if axis == 0 {
                                        r.rect.map(|q| q.width as f32).unwrap_or(rect.width())
                                    } else {
                                        r.rect.map(|q| q.height as f32).unwrap_or(rect.height())
                                    } / dimensions[axis].max(1) as f32;
                            }
                        }
                        let c = r.visual.tint;
                        let xs = texture_axis(
                            uv[0],
                            uv[1],
                            if r.visual.tile[0] && r.visual.wrap_modes[0] == "CLAMP" {
                                "REPEAT"
                            } else {
                                &r.visual.wrap_modes[0]
                            },
                        );
                        let ys = texture_axis(
                            uv[2],
                            uv[3],
                            if r.visual.tile[1] && r.visual.wrap_modes[1] == "CLAMP" {
                                "REPEAT"
                            } else {
                                &r.visual.wrap_modes[1]
                            },
                        );
                        for &(x0, x1, u0, u1) in &xs {
                            for &(y0, y1, v0, v1) in &ys {
                                painter.image(
                                    texture.id(),
                                    ERect::from_min_max(
                                        Pos2::new(
                                            rect.left() + x0 * rect.width(),
                                            rect.top() + y0 * rect.height(),
                                        ),
                                        Pos2::new(
                                            rect.left() + x1 * rect.width(),
                                            rect.top() + y1 * rect.height(),
                                        ),
                                    ),
                                    ERect::from_min_max(Pos2::new(u0, v0), Pos2::new(u1, v1)),
                                    fade(
                                        r,
                                        Color32::from_rgba_unmultiplied(
                                            (c[0] * 255.0) as u8,
                                            (c[1] * 255.0) as u8,
                                            (c[2] * 255.0) as u8,
                                            (c[3] * 255.0) as u8,
                                        ),
                                    ),
                                );
                            }
                        }
                        continue;
                    }
                    // The same missing-resource symbol at every size, with no
                    // frame names or clipped labels mistaken for client art.
                    painter.rect_filled(rect, 0.0, fade(r, Color32::from_rgb(46, 35, 49)));
                    let middle = rect.center();
                    for tile in [
                        ERect::from_min_max(rect.min, middle),
                        ERect::from_min_max(middle, rect.max),
                    ] {
                        painter.rect_filled(tile, 0.0, fade(r, Color32::from_rgb(132, 63, 127)));
                    }
                    painter.line_segment(
                        [rect.left_top(), rect.right_bottom()],
                        Stroke::new(1.0, fade(r, Color32::from_rgb(218, 130, 210))),
                    );
                    painter.line_segment(
                        [rect.right_top(), rect.left_bottom()],
                        Stroke::new(1.0, fade(r, Color32::from_rgb(218, 130, 210))),
                    );
                }
                (None, "FontString" | "EditBox") => {
                    let inset = r.visual.text_insets;
                    let factor = r
                        .text_layout
                        .as_ref()
                        .map(|l| r.visual.size / l.font_size)
                        .unwrap_or(1.0) as f32
                        * scale;
                    let text_rect = ERect::from_min_max(
                        rect.min + egui::vec2(inset[0] as f32, inset[2] as f32) * factor,
                        rect.max - egui::vec2(inset[1] as f32, inset[3] as f32) * factor,
                    );
                    let text_painter =
                        painter.with_clip_rect(text_rect.intersect(painter.clip_rect()));
                    crate::text_render::TextPainter {
                        ctx: ui.ctx(),
                        assets: &self.assets,
                        glyphs: &mut self.glyphs,
                        textures: &mut self.textures,
                    }
                    .paint(
                        &text_painter,
                        r,
                        text_rect.translate(
                            egui::vec2(
                                -r.visual.text_scroll.0 as f32,
                                -r.visual.text_scroll.1 as f32,
                            ) * factor,
                        ),
                        scale,
                    );
                }
                (None, "Texture") => {}
                _ => {}
            }
        }
        // Selection and hover outlines on top.
        let hit = |pos: Pos2| {
            ordered
                .iter()
                .rev()
                .filter(|r| visible(r))
                .find(|r| {
                    r.rect.as_ref().is_some_and(|x| to_screen(x).contains(pos))
                        && r.clip.as_ref().is_none_or(|x| to_screen(x).contains(pos))
                })
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
        let events = preview_events(
            ui,
            &response,
            origin,
            scale,
            uh,
            self.in_flight,
            &mut self.preview_input,
        );
        if !events.is_empty() {
            self.act(Command::Input { events });
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
        // The tab draws into exactly the space left in the panel, and the
        // panel claims exactly that space: content can neither shrink the
        // panel nor grow it.
        let area = ui.available_rect_before_wrap();
        let mut content = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(Align::Min)),
        );
        content.set_clip_rect(area.intersect(ui.clip_rect()));
        match self.tab {
            Tab::Problems => self.editor.ui_problems(&mut content),
            Tab::Console => self.console_tab(&mut content),
            Tab::Events => self.events_tab(&mut content),
        }
        ui.allocate_rect(area, Sense::hover());
    }

    /// A fixed-width table cell with its content left-aligned
    /// (`add_sized` would centre it).
    fn cell(ui: &mut egui::Ui, width: f32, widget: impl egui::Widget) {
        ui.allocate_ui_with_layout(
            egui::vec2(width, 18.0),
            egui::Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(width);
                ui.add(widget);
            },
        );
    }

    fn chip(ui: &mut egui::Ui, on: bool, label: &str) -> bool {
        let text = egui::RichText::new(label).size(12.0).color(if on {
            Color32::WHITE
        } else {
            palette::MUTED
        });
        let button = egui::Button::new(text)
            .fill(if on {
                palette::ACCENT_SOFT
            } else {
                palette::RAISED
            })
            .corner_radius(10.0)
            .min_size(egui::vec2(0.0, 22.0));
        ui.add(button).clicked()
    }

    fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str) {
        ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(200.0)
                .margin(egui::Margin::symmetric(8, 3)),
        );
    }

    fn console_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (f, label) in [
                (ConsoleFilter::All, "All"),
                (ConsoleFilter::Output, "Output"),
                (ConsoleFilter::Errors, "Errors"),
                (ConsoleFilter::Logs, "Logs"),
            ] {
                if Self::chip(ui, self.console_filter == f, label) {
                    self.console_filter = f;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                Self::search_box(ui, &mut self.console_search, "Filter output");
            });
        });
        ui.add_space(4.0);
        let input_height = 34.0;
        let list_height = (ui.available_height() - input_height - 6.0).max(40.0);
        let needle = self.console_search.to_lowercase();
        egui::Frame::new()
            .fill(palette::BG)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("console")
                    .max_height(list_height - 12.0)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let mut shown = 0;
                        for l in &self.console {
                            let keep = match self.console_filter {
                                ConsoleFilter::All => true,
                                ConsoleFilter::Output => {
                                    matches!(l.kind, LineKind::Print | LineKind::Input)
                                }
                                ConsoleFilter::Errors => l.kind == LineKind::Error,
                                ConsoleFilter::Logs => matches!(
                                    l.kind,
                                    LineKind::Log | LineKind::Build | LineKind::Separator
                                ),
                            } && (needle.is_empty() || l.text.to_lowercase().contains(&needle));
                            if !keep {
                                continue;
                            }
                            shown += 1;
                            Self::console_row(ui, l);
                        }
                        if shown == 0 {
                            ui.label(
                                egui::RichText::new(if self.console.is_empty() {
                                    "Nothing printed yet. The addon's print() output, errors and notes appear here."
                                } else {
                                    "No lines match the filter."
                                })
                                .color(palette::FAINT),
                            );
                        }
                    });
            });
        ui.add_space(6.0);
        // Input at the bottom, as in a terminal.
        let mut submit = false;
        egui::Frame::new()
            .fill(palette::RAISED)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("›").size(16.0).color(palette::ACCENT));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.input)
                            .id(egui::Id::unique("console-input"))
                            .hint_text("Run Lua, or a slash command such as /reload")
                            .frame(egui::Frame::NONE)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                    if r.has_focus() {
                        ui.input_mut(|i| {
                            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                                && !self.history.is_empty()
                            {
                                self.history_pos = Some(
                                    self.history_pos
                                        .map_or(self.history.len() - 1, |p| p.saturating_sub(1)),
                                );
                            }
                            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                                self.history_pos = self
                                    .history_pos
                                    .map(|p| p + 1)
                                    .filter(|p| *p < self.history.len());
                                if self.history_pos.is_none() {
                                    self.input.clear();
                                }
                            }
                        });
                        if let Some(p) = self.history_pos
                            && let Some(h) = self.history.get(p)
                            && self.input != *h
                        {
                            self.input = h.clone();
                        }
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                        r.request_focus();
                    }
                });
            });
        if submit && !self.input.trim().is_empty() {
            let text = std::mem::take(&mut self.input);
            self.history_pos = None;
            self.line(LineKind::Input, text.clone());
            self.history.push(text.clone());
            if text.trim_start().starts_with('/') {
                self.act(Command::Slash { line: text });
            } else {
                self.act(Command::Eval { code: text });
            }
        }
    }

    fn console_row(ui: &mut egui::Ui, l: &Line) {
        if l.kind == LineKind::Separator {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, 16.0), Sense::hover());
                let p = ui.painter();
                let g =
                    p.layout_no_wrap(l.text.clone(), FontId::proportional(11.5), palette::MUTED);
                let mid = r.center();
                let half = g.size().x / 2.0 + 10.0;
                p.line_segment(
                    [egui::pos2(r.left(), mid.y), egui::pos2(mid.x - half, mid.y)],
                    Stroke::new(1.0, palette::BORDER),
                );
                p.line_segment(
                    [
                        egui::pos2(mid.x + half, mid.y),
                        egui::pos2(r.right(), mid.y),
                    ],
                    Stroke::new(1.0, palette::BORDER),
                );
                p.galley(mid - g.size() / 2.0, g, palette::MUTED);
            });
            ui.add_space(4.0);
            return;
        }
        let (glyph, glyph_color, text_color, tint) = match l.kind {
            LineKind::Print => ("", palette::FAINT, palette::TEXT, None),
            LineKind::Error => (
                icon::ERROR,
                palette::ERROR,
                Color32::from_rgb(0xff, 0xb4, 0xb0),
                Some(palette::ERROR),
            ),
            LineKind::Log if l.text.starts_with("not simulated") => {
                (icon::WARNING, palette::WARNING, palette::MUTED, None)
            }
            LineKind::Log => ("", palette::FAINT, palette::MUTED, None),
            LineKind::Input => (
                "›",
                palette::ACCENT,
                Color32::from_rgb(0xc9, 0xbd, 0xff),
                None,
            ),
            LineKind::Build => (icon::BUILD, palette::OK, palette::MUTED, None),
            LineKind::Separator => unreachable!(),
        };
        let frame = egui::Frame::new()
            .fill(tint.map_or(Color32::TRANSPARENT, |c| c.gamma_multiply(0.10)))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(4, 2));
        frame.show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                Self::cell(
                    ui,
                    56.0,
                    egui::Label::new(
                        egui::RichText::new(format!("{:>7.2}s", l.time))
                            .monospace()
                            .size(11.5)
                            .color(palette::FAINT),
                    ),
                );
                let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), Sense::hover());
                if !glyph.is_empty() {
                    let font = if glyph == "›" {
                        FontId::proportional(15.0)
                    } else {
                        icon_font(11.0)
                    };
                    ui.painter()
                        .text(r.center(), Align2::CENTER_CENTER, glyph, font, glyph_color);
                }
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&l.text)
                            .monospace()
                            .size(13.0)
                            .color(text_color),
                    )
                    .wrap()
                    .selectable(true),
                );
            });
        });
    }

    fn events_tab(&mut self, ui: &mut egui::Ui) {
        // Fire bar.
        egui::Frame::new()
            .fill(palette::RAISED)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Fire event").color(palette::MUTED));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.event_name)
                            .hint_text("EVENT_NAME")
                            .font(egui::TextStyle::Monospace)
                            .desired_width(220.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.event_args)
                            .hint_text("arguments: player 42 true \"two words\"")
                            .font(egui::TextStyle::Monospace)
                            .desired_width(300.0),
                    );
                    let fire = egui::Button::new(egui::RichText::new("Fire").color(Color32::WHITE))
                        .fill(palette::ACCENT);
                    if ui
                        .add_enabled(!self.event_name.trim().is_empty(), fire)
                        .clicked()
                    {
                        let event = self.event_name.trim().to_string();
                        let args = parse_args(&self.event_args);
                        self.act(Command::Fire { event, args });
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("Clear").frame(false)).clicked() {
                            self.trace.clear();
                        }
                    });
                });
            });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (f, label) in [
                (EventFilter::All, "All"),
                (EventFilter::Events, "Events"),
                (EventFilter::Output, "Prints"),
                (EventFilter::Timers, "Timers & scripts"),
                (EventFilter::Errors, "Errors"),
            ] {
                if Self::chip(ui, self.event_filter == f, label) {
                    self.event_filter = f;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                Self::search_box(ui, &mut self.event_search, "Filter by name or argument");
            });
        });
        ui.add_space(4.0);
        let needle = self.event_search.to_lowercase();
        egui::Frame::new()
            .fill(palette::BG)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                // Header.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (w, h) in [(64.0, "TIME"), (96.0, "KIND"), (260.0, "NAME"), (0.0, "ARGUMENTS")] {
                        let t = egui::RichText::new(h).size(10.5).strong().color(palette::FAINT);
                        if w > 0.0 {
                            Self::cell(ui, w, egui::Label::new(t));
                        } else {
                            ui.label(t);
                        }
                    }
                });
                crate::theme::rule(ui);
                egui::ScrollArea::vertical()
                    .id_salt("events")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        use selu_sim::TraceKind as K;
                        let mut shown = 0;
                        for (i, r) in self.trace.iter().enumerate() {
                            let keep = match self.event_filter {
                                EventFilter::All => true,
                                EventFilter::Events => matches!(r.kind, K::Event | K::Host),
                                EventFilter::Output => r.kind == K::Print,
                                EventFilter::Timers => {
                                    matches!(r.kind, K::Timer | K::Script | K::Hook)
                                }
                                EventFilter::Errors => {
                                    matches!(r.kind, K::Error | K::NotSimulated)
                                }
                            };
                            let hay = format!("{} {}", r.target, r.args.join(" ")).to_lowercase();
                            if !keep || (!needle.is_empty() && !hay.contains(&needle)) {
                                continue;
                            }
                            shown += 1;
                            let (label, color) = match r.kind {
                                K::Event => ("event", palette::ACCENT),
                                K::Host => ("command", Color32::from_rgb(0x4f, 0xc1, 0xff)),
                                K::Print => ("print", palette::MUTED),
                                K::Timer => ("timer", palette::WARNING),
                                K::Script => ("script", palette::OK),
                                K::Hook => ("hook", palette::OK),
                                K::ProtectedCall => ("protected", Color32::from_rgb(0xd7, 0xba, 0x7d)),
                                K::Deferred => ("deferred", Color32::from_rgb(0xd7, 0xba, 0x7d)),
                                K::Error => ("error", palette::ERROR),
                                K::NotSimulated => ("not simulated", palette::WARNING),
                            };
                            let row = egui::Frame::new()
                                .fill(if i % 2 == 0 {
                                    Color32::TRANSPARENT
                                } else {
                                    Color32::from_white_alpha(4)
                                })
                                .inner_margin(egui::Margin::symmetric(0, 2));
                            row.show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    Self::cell(ui, 64.0,
                                        egui::Label::new(
                                            egui::RichText::new(format!("{:>7.3}", r.time))
                                                .monospace()
                                                .size(11.5)
                                                .color(palette::FAINT),
                                        ),
                                    );
                                    // Kind pill.
                                    let (cell, _) =
                                        ui.allocate_exact_size(egui::vec2(96.0, 18.0), Sense::hover());
                                    let p = ui.painter();
                                    let g = p.layout_no_wrap(
                                        label.to_string(),
                                        FontId::proportional(11.0),
                                        color,
                                    );
                                    let pill = egui::Rect::from_min_size(
                                        cell.left_center() - egui::vec2(0.0, 9.0),
                                        egui::vec2(g.size().x + 14.0, 18.0),
                                    );
                                    p.rect_filled(pill, 9.0, color.gamma_multiply(0.18));
                                    p.galley(pill.center() - g.size() / 2.0, g, color);
                                    let (name, args) = if r.kind == K::Print {
                                        (String::new(), r.args.join("\t"))
                                    } else {
                                        (r.target.clone(), r.args.join(", "))
                                    };
                                    Self::cell(ui, 260.0,
                                        egui::Label::new(
                                            egui::RichText::new(name)
                                                .monospace()
                                                .size(13.0)
                                                .strong()
                                                .color(palette::TEXT),
                                        )
                                        .truncate(),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(args)
                                                .monospace()
                                                .size(13.0)
                                                .color(if r.kind == K::Print {
                                                    palette::TEXT
                                                } else {
                                                    palette::MUTED
                                                }),
                                        )
                                        .truncate(),
                                    );
                                });
                            });
                        }
                        if shown == 0 {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(if self.trace.is_empty() {
                                    "No events yet. Events, prints, timers and errors appear here in order."
                                } else {
                                    "No entries match the filter."
                                })
                                .color(palette::FAINT),
                            );
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use selu_sim::RegionId;

    #[test]
    fn preview_rejects_empty_or_inverted_quads() {
        // Missing-art diagonals must not turn an empty three-slice center
        // into visible artwork. Textured quads use this same geometry gate.
        for size in [
            Vec2::new(-35., 16.),
            Vec2::new(16., -35.),
            Vec2::new(0., 16.),
            Vec2::new(16., 0.),
            Vec2::new(f32::INFINITY, 16.),
            Vec2::ZERO,
        ] {
            assert!(paint_rect(ERect::from_min_size(Pos2::ZERO, size)).is_none());
        }
        assert!(paint_rect(ERect::from_min_size(Pos2::ZERO, Vec2::splat(16.))).is_some());
    }

    #[test]
    fn preview_only_forwards_input_when_it_owns_the_pointer() {
        let ctx = egui::Context::default();
        let mut state = PreviewInputState::default();
        let mut events = Vec::new();
        let pointer = Pos2::new(100., 100.);
        for raw in [
            vec![egui::Event::PointerMoved(pointer)],
            vec![egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(ERect::from_min_size(Pos2::ZERO, Vec2::new(400., 300.))),
                    events: raw,
                    ..Default::default()
                },
                |root| {
                    root.add_enabled_ui(false, |ui| {
                        let response = ui.allocate_rect(
                            ERect::from_min_size(Pos2::ZERO, Vec2::new(400., 300.)),
                            Sense::click_and_drag(),
                        );
                        events.extend(preview_events(
                            ui,
                            &response,
                            Pos2::ZERO,
                            1.,
                            300.,
                            0,
                            &mut state,
                        ));
                    });
                },
            );
            output.textures_delta.clear();
        }
        assert!(
            !events.iter().any(|event| matches!(
                event,
                PreviewInput::MouseButton { .. } | PreviewInput::Hover { x: 0.0.., .. }
            )),
            "{events:?}"
        );
    }

    #[test]
    #[ignore = "requires read-only matching client and UI sources"]
    fn client_preview_routes_pointer_phases_hover_wheel_and_live_drag() {
        let worker = Worker::spawn(selu_sim::SessionConfig {
            target: Some("wowf@1.60.1.70790".into()),
            client_install: Some(
                std::env::var_os("SELU_CLIENT_INSTALL")
                    .expect("client install")
                    .into(),
            ),
            ..Default::default()
        });
        let call = |command| {
            let updates = worker.call(command);
            assert!(
                !updates.iter().any(|u| matches!(u, Update::Error { .. })),
                "{updates:?}"
            );
            updates
        };
        call(Command::LoadDefaultUi {
            source: std::env::var_os("SELU_DEFAULT_UI_SOURCE")
                .expect("UI source")
                .into(),
        });
        call(Command::Login);
        let snapshot = || {
            call(Command::Snapshot)
                .into_iter()
                .find_map(|u| {
                    if let Update::Snapshot { snapshot } = u {
                        Some(*snapshot)
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        let center = |name: &str| {
            let s = snapshot();
            let r = s
                .regions
                .iter()
                .find(|r| r.name.as_deref() == Some(name))
                .unwrap();
            assert!(r.visible);
            let q = r.rect.unwrap();
            Pos2::new(
                40. + (q.left + q.width / 2.) as f32 * 0.5,
                60. + (1080. - q.bottom - q.height / 2.) as f32 * 0.5,
            )
        };
        let ctx = egui::Context::default();
        let mut state = PreviewInputState::default();
        let mut frame = |events: Vec<egui::Event>| {
            let mut result = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(ERect::from_min_size(Pos2::ZERO, Vec2::new(1100., 700.))),
                    events,
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(root, |ui| {
                            let response = ui.allocate_rect(
                                ERect::from_min_size(Pos2::new(40., 60.), Vec2::new(960., 540.)),
                                Sense::click_and_drag(),
                            );
                            result = preview_events(
                                ui,
                                &response,
                                response.rect.min,
                                0.5,
                                1080.,
                                0,
                                &mut state,
                            );
                        });
                },
            );
            output.textures_delta.clear();
            call(Command::Input { events: result });
        };
        frame(vec![]);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let bag = center("MainMenuBarBackpackButton");
        frame(vec![egui::Event::PointerMoved(bag), button(bag, true)]);
        frame(vec![button(bag, false)]);
        assert!(
            snapshot()
                .regions
                .iter()
                .any(|r| r.name.as_deref() == Some("ContainerFrame1") && r.visible)
        );
        let micro = center("CharacterMicroButton");
        frame(vec![egui::Event::PointerMoved(micro)]);
        call(Command::Eval {
            code:
                "assert(GameTooltip:IsVisible() and GameTooltip:GetOwner()==CharacterMicroButton)"
                    .into(),
        });
        call(Command::Eval{code:"CloseAllBags();for i=1,40 do print('studio scroll '..i) end;ChatFrame1:ScrollToBottom()".into()});
        let chat = center("ChatFrame1");
        frame(vec![
            egui::Event::PointerMoved(chat),
            egui::Event::MouseWheel {
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0., 1.),
                modifiers: Modifiers::NONE,
            },
        ]);
        call(Command::Eval{code:"assert(ChatFrame1:GetScrollOffset()>0);local f=CreateFrame('Frame','StudioLiveDrag');f:SetSize(60,40);f:SetPoint('BOTTOMLEFT',900,700);f:EnableMouse(true);f:SetMovable(true);f:RegisterForDrag('LeftButton');f:SetScript('OnDragStart',function(self)self:StartMoving()end);f:SetScript('OnDragStop',function(self)self:StopMovingOrSizing()end)".into()});
        let from = center("StudioLiveDrag");
        frame(vec![egui::Event::PointerMoved(from), button(from, true)]);
        let to = from + Vec2::new(10., -5.);
        frame(vec![egui::Event::PointerMoved(to)]);
        call(Command::Eval {
            code: "assert(StudioLiveDrag:GetLeft()==920 and StudioLiveDrag:GetBottom()==710)"
                .into(),
        });
        let end = from + Vec2::new(20., -15.);
        frame(vec![egui::Event::PointerMoved(end)]);
        call(Command::Eval {
            code: "assert(StudioLiveDrag:GetLeft()==940 and StudioLiveDrag:GetBottom()==730)"
                .into(),
        });
        frame(vec![
            button(end, false),
            egui::Event::PointerMoved(Pos2::new(100., 100.)),
        ]);
        call(Command::Eval {
            code: "assert(StudioLiveDrag:GetLeft()==940 and StudioLiveDrag:GetBottom()==730)"
                .into(),
        });
    }

    #[test]
    fn saved_folder_choices_use_fixture_tree() {
        let root = std::env::temp_dir().join(format!("studio-sl29-{}", std::process::id()));
        for part in ["Account/B", "Account/A", "Account/SavedVariables"] {
            std::fs::create_dir_all(root.join(part)).unwrap();
        }
        assert_eq!(saved_dirs(&root.join("Account")).unwrap(), ["A", "B"]);
        assert!(
            saved_dirs(&root.join("Missing"))
                .unwrap_err()
                .contains("Missing")
        );
        std::fs::remove_dir_all(root).unwrap();
        let mut settings = crate::game::Settings::default();
        settings.wtf_override = "fixture/WTF".into();
        settings.saved_choices.insert(
            "fixture-project".into(),
            vec!["A".into(), "Realm".into(), "Hero".into()],
        );
        let encoded = toml::to_string(&settings).unwrap();
        let decoded: crate::game::Settings = toml::from_str(&encoded).unwrap();
        assert_eq!(settings, decoded);
    }

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
            text_layout: None,
            visual: Default::default(),
            clip: None,
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

/// Empty geometry has no drawable pixels, including diagnostic markers.
fn paint_rect(rect: ERect) -> Option<ERect> {
    // An inverted three-slice center is empty, not a mirrored quad. In
    // particular, the missing-art diagonals must not draw across its edges.
    (rect.is_finite() && rect.is_positive()).then_some(rect)
}

/// Split at sampler boundaries so each axis can clamp, repeat or mirror independently.
fn texture_axis(start: f32, end: f32, mode: &str) -> Vec<(f32, f32, f32, f32)> {
    if !start.is_finite() || !end.is_finite() {
        return vec![];
    }
    let mut cuts = vec![0.0, 1.0];
    if start != end {
        let low = start.min(end);
        let high = start.max(end);
        // Bound geometry for adversarial UVs.
        for boundary in (low.floor() as i32).max(-256)..=(high.ceil() as i32).min(256) {
            if mode == "CLAMP" && boundary != 0 && boundary != 1 {
                continue;
            }
            let t = (boundary as f32 - start) / (end - start);
            if t > 0.0 && t < 1.0 {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f32::total_cmp);
    cuts.windows(2)
        .map(|pair| {
            let a = start + (end - start) * pair[0];
            let b = start + (end - start) * pair[1];
            let cell = ((a + b) / 2.0).floor();
            let map = |v: f32| match mode {
                "REPEAT" => v - cell,
                "MIRROR" => {
                    if cell.rem_euclid(2.0) == 0.0 {
                        v - cell
                    } else {
                        1.0 - (v - cell)
                    }
                }
                _ => v.clamp(0.0, 1.0),
            };
            (pair[0], pair[1], map(a), map(b))
        })
        .collect()
}

#[cfg(test)]
mod sim4_texture_tests {
    use super::texture_axis;
    #[test]
    fn sampler_boundaries_preserve_repeat_mirror_and_clamp() {
        assert_eq!(
            texture_axis(0.0, 2.0, "REPEAT"),
            vec![(0.0, 0.5, 0.0, 1.0), (0.5, 1.0, 0.0, 1.0)]
        );
        assert_eq!(
            texture_axis(0.0, 2.0, "MIRROR"),
            vec![(0.0, 0.5, 0.0, 1.0), (0.5, 1.0, 1.0, 0.0)]
        );
        assert_eq!(
            texture_axis(-1.0, 1.0, "CLAMP"),
            vec![(0.0, 0.5, 0.0, 0.0), (0.5, 1.0, 0.0, 1.0)]
        );
    }
}

#[cfg(test)]
mod open_project_tests {
    use super::find_project;

    #[test]
    fn plain_addon_target_follows_interface_numbers_not_one_product() {
        use super::*;
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json");
        let base = selu_manifest::load_manifest(&fixture).unwrap();
        let make = |flavour: &str, interface: u32, build: &str| {
            let mut m = base.clone();
            m.fixture = false;
            m.flavour = flavour.into();
            m.interface = interface;
            m.build = selu_manifest::Build::parse(build).unwrap();
            m
        };
        let all = vec![
            make("wow", 120005, "12.0.5.1"),
            make("wowf", 16001, "1.60.1.1"),
            make("wow_classic_era", 11508, "1.15.8.1"),
            make("wow_classic", 50504, "5.5.4.1"),
        ];
        let pick = |interfaces: &[u32], fallback: Option<usize>| {
            pick_plain_target(&all, interfaces, fallback.map(|i| &all[i]))
                .map(|m| m.flavour.clone())
        };
        // The first listed interface decides; no product is assumed.
        assert_eq!(
            pick(&[11508, 16001], None).as_deref(),
            Some("wow_classic_era")
        );
        assert_eq!(pick(&[50504], None).as_deref(), Some("wow_classic"));
        // The Studio setting wins when its product is among the matches.
        assert_eq!(pick(&[11508, 16001], Some(1)).as_deref(), Some("wowf"));
        // Without an exact match: the setting, else the nearest interface.
        assert_eq!(pick(&[11509], Some(3)).as_deref(), Some("wow_classic"));
        assert_eq!(pick(&[120007], None).as_deref(), Some("wow"));
        assert_eq!(pick(&[], None), None);
    }

    #[test]
    fn plain_addon_discovery_target_load_error_and_saved_edit_reload() {
        use super::*;
        let root = std::env::temp_dir().join(format!("sl26-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(format!("sl26-{}.toc", std::process::id())),
            "## Interface: 10000\nDemo.lua\n",
        )
        .unwrap();
        std::fs::write(root.join("Demo.lua"), "value = 1").unwrap();
        assert_eq!(find_project(&root), Some(root.clone()));
        assert_eq!(built_addon(&root), Some(root.clone()));
        let manifests = root.join("manifests");
        std::fs::create_dir_all(&manifests).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/fixtures/forever-1.0.0.1000.json");
        let mut m = selu_manifest::load_manifest(&fixture).unwrap();
        m.fixture = false;
        m.interface = 10000;
        std::fs::write(manifests.join("test.json"), serde_json::to_vec(&m).unwrap()).unwrap();
        assert_eq!(
            addon_manifest(&root, &manifests, None)
                .unwrap()
                .target_string(),
            m.target_string()
        );
        let mut session = selu_sim::Session::new(SessionConfig::default()).unwrap();
        session.load_addon_dir(&root).unwrap();
        session.eval("assert(value == 1)").unwrap();
        let ctx = egui::Context::default();
        let mut editor = crate::editor::Editor::new(Some(root.clone()), None, None, &ctx);
        let before_save = sources_stamp(&root);
        editor.open[0].text = "value = 2".into();
        editor.save_all();
        assert_ne!(
            sources_stamp(&root),
            before_save,
            "save must trigger the Studio source watcher"
        );
        session.reload().unwrap();
        session.eval("assert(value == 2)").unwrap();
        std::fs::write(root.join("Demo.lua"), "error('broken')").unwrap();
        let mut broken = selu_sim::Session::new(SessionConfig::default()).unwrap();
        let error = broken.load_addon_dir(&root).unwrap_err().to_string();
        assert!(error.contains(":1:"), "{error}");
        std::fs::write(root.join("Demo.lua"), "value = 3").unwrap();
        broken.reload().unwrap();
        broken.eval("assert(value == 3)").unwrap();
        editor.runtime_error(&error);
        assert_eq!(editor.problems[0].line, 1);
        editor.problems.clear();
        editor.runtime_error(&format!("{}:7: broken", root.join("Demo.lua").display()));
        assert_eq!(editor.problems[0].line, 7);
        assert_eq!(editor.problems[0].file.as_deref(), Some("Demo.lua"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn finds_the_project_in_the_folder_its_selu_folder_or_one_subfolder() {
        let root = std::env::temp_dir().join(format!("selu-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // A worktree whose project lives in `selu/` (the toolkit port layout).
        let worktree = root.join("worktree");
        std::fs::create_dir_all(worktree.join("selu")).unwrap();
        std::fs::write(worktree.join("selu").join("selu.toml"), "").unwrap();
        assert_eq!(find_project(&worktree), Some(worktree.join("selu")));
        // The project folder itself.
        assert_eq!(
            find_project(&worktree.join("selu")),
            Some(worktree.join("selu"))
        );
        // Exactly one subfolder with a project.
        let parent = root.join("parent");
        std::fs::create_dir_all(parent.join("addon")).unwrap();
        std::fs::write(parent.join("addon").join("selu.toml"), "").unwrap();
        assert_eq!(find_project(&parent), Some(parent.join("addon")));
        // No project anywhere.
        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(find_project(&empty), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// The subfolders of a WTF account/realm folder, sorted; an unreadable
/// folder says why.
fn saved_dirs(path: &Path) -> Result<Vec<String>, String> {
    let entries =
        std::fs::read_dir(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut names: Vec<_> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|s| s != "SavedVariables")
        .collect();
    names.sort();
    Ok(names)
}
