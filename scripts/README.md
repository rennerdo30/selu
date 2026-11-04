# Scripts

Windows batch files for everyday jobs. Run them from any folder; each one
builds the release `selu` and `selu-studio` first (into `target\release`),
so they always use the current sources. None of them change `PATH` or any
other setting.

| Script | What it does |
|---|---|
| `studio.bat [project] [--target t]` | Opens Selu Studio for a Selu project (`selu debug`): builds it, opens the window, and rebuilds and reloads when a file under `src` changes. Default: `examples\frames`. |
| `studio-demo.bat` | Opens Studio on a hand-written demo addon (a panel with a bar that grows): press Play, or type `/demo hide` in the console. |
| `studio-headless.bat [addon folder]` | Runs the Studio engine without a window on a built addon folder; type JSON commands, one per line (examples in the file). |
| `build.bat [project] [--target t]` | Builds a project into its `dist` folder (`selu build`). |
| `test.bat [project] [--filter text]` | Runs a project's `*.test.selu` files (`selu test`; the runner comes in round 9). |
| `check.bat` | The gate before a push: fmt, clippy, all workspace tests, the proof tests. Slow. |

`_tools.bat` is the shared build step the others call.

## Studio at a glance

- **Code** (the default view for a project): the project's files on the
  left, tabs, syntax colours, line numbers. Errors appear while you type,
  as red or yellow underlines and in the Problems tab (click one to jump
  there). Completion pops up as you type (game API functions, `C_` namespace
  members after a dot, events after `on`, keywords and your own names);
  Ctrl+Space opens it, Up/Down pick, Enter or Tab accept, Esc closes. Hover a
  game API name for its signature. Ctrl+S saves every file, which also
  rebuilds and reloads the addon.
- **Game:** the simulated screen (below).
- **Toolbar:** `/reload`, Rebuild, rebuild on save, Play/Pause with speed,
  step time by 0.1, 1 or 10 seconds, Screenshot (saved under the project's
  `dist\.selu\`).
- **Frames** (left): every frame, texture and font string; click to select.
  Hidden ones show with the "hidden" box ticked.
- **Game screen** (middle): the UI at 1920 x 1080, scaled to fit. Click a
  region to select it; hover shows its name.
- **Inspector** (right): the selected region's type, name, position, size,
  texture, text and scripts.
- **Console** (bottom): the addon's prints, errors and "not simulated"
  notes. Type Lua, or a slash command such as `/demo`.
- **Events** (bottom tab): every event, print and error in order; fire any
  event with arguments (`player 42 true`).

What it can't show yet: strata, frame levels, scale, real textures and
fonts, mouse clicks into the addon, and the debugger. Those come with the
simulator packages S1 and S6 (see `docs/SIMULATOR.md`).
