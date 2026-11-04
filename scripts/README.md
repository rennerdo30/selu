# Scripts

Windows batch files for everyday jobs. Run them from any folder; each one
builds the release `selu` and `selu-studio` first (into `target\release`),
so they always use the current sources. None of them change `PATH` or any
other setting.

| Script | What it does |
|---|---|
| `studio.bat [project] [--target t]` | Opens Selu Studio for a project (`selu debug`). Default: `examples\hello`. Until round 9 (package S5) the window doesn't exist yet and Studio says so. |
| `studio-headless.bat [addon folder]` | Runs the Studio engine without a window on a built addon folder; type JSON commands, one per line (examples in the file). Default: the simulator's `Hello` test addon. |
| `build.bat [project] [--target t]` | Builds a project into its `dist` folder (`selu build`). |
| `test.bat [project] [--filter text]` | Runs a project's `*.test.selu` files (`selu test`; the runner comes in round 9). |
| `check.bat` | The gate before a push: fmt, clippy, all workspace tests, the proof tests. Slow. |

`_tools.bat` is the shared build step the others call.
