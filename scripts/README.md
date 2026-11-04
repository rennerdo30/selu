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

The window is laid out like a code editor:

- **Menu bar** (File, Edit, View, Run, Help) with the run controls in the
  middle: run or pause the game clock (F5), /reload (Ctrl+R), build (Ctrl+B)
  and the game time.
- **Activity bar** (far left): Explorer and Frames; the screen icon at the
  bottom shows or hides the game preview.
- **Explorer:** the project's folders and files, with unsaved dots and error
  counts; `+` creates a file in `src/`.
- **Editor:** tabs (middle-click or the cross closes; a dot means unsaved),
  breadcrumbs, line numbers with error markers, the current line
  highlighted. Errors appear while you type (underlined, and listed under
  Problems). Completion pops up as you type: game API functions, `C_`
  namespace members after a dot, events after `on`, keywords and your own
  names. Ctrl+Space opens it, Up/Down pick, Enter or Tab accept, Esc closes.
  Hover a game API name for its signature.
- **Game preview** (right): the simulated screen at 1920 x 1080 scaled to
  fit, and the inspector for the selected frame.
- **Bottom panel:** Problems, Console (the addon's prints and errors; type
  Lua or a slash command such as `/demo`) and Events (everything that
  happened, with "fire an event").
- **Status bar:** target, error and warning counts, cursor position, game
  time.

- **Format Document** (Shift+Alt+F, Edit menu) runs the same formatter as
  `selu fmt`; Project Settings can turn on formatting when saving.
- **Project Settings** (File menu or Ctrl+,): addon name, targets (flavour
  and build picked from the manifests), output layout, strict API checks,
  indent width and format on save. Saving edits `selu.toml` in place and
  keeps its comments.

- **Run in Game** (Ctrl+F5, Run menu, or the controller button in the title
  bar): builds the project, copies the built addon into the matching game
  client's `Interface\AddOns` and starts that client through Battle.net, so
  you're logged in (or, if it's already
  running, tells you to type `/reload` in game). Studio finds the installs
  through Windows' uninstall entries and the usual folders on every drive;
  **Run > Game Installs** shows them, lets you choose the client per
  flavour, override a flavour and add an install folder by hand. Studio only
  replaces addon folders it copied itself (they hold a `.selu-studio`
  marker); for any other folder of the same name it asks, and the old folder
  goes to the Recycle Bin. `selu-studio --list-games` prints what it finds.
  Choices are saved in `%APPDATA%\Selu\studio.toml`.

Shortcuts: Ctrl+F5 run in game, Ctrl+P open a file, Ctrl+S save all (rebuilds and reloads),
Shift+Alt+F format, Ctrl+, project settings,
Ctrl+W close the tab, Ctrl+F find, Ctrl+= / Ctrl+- / Ctrl+0 zoom, F5 run or
pause, Ctrl+R /reload, Ctrl+B build, Ctrl+J bottom panel, Ctrl+Shift+E side
bar, Ctrl+Shift+V game preview. Help > Keyboard Shortcuts lists them.

Without a project (`studio-demo.bat`) the game screen is the main view.

What it can't do yet: go to definition, rename, find in all files, strata
and frame levels in the preview, real textures, clicks into the addon, and
the debugger.
