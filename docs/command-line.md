# Command-line options and the log

## Options

| Option | What it does |
| --- | --- |
| (none) | Attaches to Explorer and draws the boxes. |
| `--tray-only` | Does not attach; uses only the tray icon and the popup. |
| `--remove` | Removes what it left on this PC and exits; `uninstall.exe` runs this ([Uninstall](uninstall.md)). |
| `--diagnostics` | Adds detailed lines from the DLL inside Explorer to the log. |
| `--look=slim\|inline\|stack\|pie\|rings` | Chooses the first box's look. |
| `--fill=used\|left` | Whether values show the amount used or the amount left, the same as `Show values as` in the settings window: in every box, the popup and the notifications. |
| `--background=box\|cards` | The first box's background: one box or a card for each service. |
| `--place=<place>` | The first box's place: `tray` (beside the tray, the default), `left` (the left end of the taskbar), `before-start` (just before the Start button), `after-apps` (just after the last app). |
| `--claude-fallback` | Reads Claude only through `/usage` in a hidden console, without `get_usage`. |

`--look`, `--fill`, `--background` and `--place` win over what is saved.

- An option LimitPeeker does not know, or a value it does not know (such as `--look=card`), shows a message listing the
  options, and LimitPeeker does not start.
- While LimitPeeker is running, starting it again does nothing, and the options given then are not used (an option it
  does not know still shows its message). Close it first.

These are modes the hidden copies of the tools that LimitPeeker starts run as their status line command. There is no need
to use them yourself. Each works only as the one option given.

| Option | What it does |
| --- | --- |
| `--claude-statusline` | The status line command of the hidden Claude Code that Claude's fallback starts, not of the Claude Code you use. It writes the time, the start of the session's id and the `5h` and `7d` limits it is handed to `%LOCALAPPDATA%\LimitPeeker\claude-heard.log` and prints one line. The file moves to `claude-heard.1.log` once it passes 256 KB. |
| `--agy-statusline` | The status line command of the hidden Antigravity CLI. It writes only the group names, the shares left and the reset times of the `quota` it is handed to `%LOCALAPPDATA%\LimitPeeker\agy-heard.log` and prints one line. |

Options for development and diagnosis:

| Option | What it does |
| --- | --- |
| `--mock` | Shows mock values for the four services and PC instead of reading them. |
| `--cycle-mock` | Changes the mock values every 4 seconds (normal; 0 and 100; not read yet, failed, and a service not read at all; Codex without its 5-hour limit), to see that the box keeps its width when only the values change. |
| `--sample-boxes` | For tests: shows four boxes with one service each (Claude beside the tray, Codex at the left end, Antigravity before the Start button, PC after the last app). Nothing is saved, and dropping a box does not move it. |
| `--agy-probe` | Reads Antigravity once the way LimitPeeker does, without the tray, and leaves the result and the time it took in `agy-probe-date-time.txt` next to the EXE, and the hidden console's last screen in `…-screen.txt`. It does nothing while LimitPeeker is running. |

## Log

`limitpeeker.log` next to the EXE records the start time and the command line, attaching, reads and failures. A read's
line also has the time it took, the CPU time the tool used and its peak memory. Sign-in data and the tools' raw answers
are never written; the plan's name (such as `plan=pro`) and the full path of each tool found are. Each notification sent
is a `notify:` line, and each tool update an `update` line. At the next run, the earlier logs move down to
`limitpeeker.1.log` through `limitpeeker.9.log`, keeping ten runs. If the EXE's folder cannot be written to (for example
under `C:\Program Files`), set the environment variable `LIMITPEEKER_LOG_DIR` to a folder that already exists, and the
log goes there; if the folder is not there, the log stays next to the EXE.

Near the top, a `job:` line says whether LimitPeeker was started inside another program's job object. Started from a
terminal inside a packaged (MSIX) app that keeps what it starts in its job, it names that app's package: Windows then
closes LimitPeeker without notice whenever that app is updated, and records it as an application hang. Start it from the
Start menu, File Explorer, a Command Prompt opened with Win+R, or at sign-in instead. If the tray icon's thread stops
taking messages for more than 2 seconds, a `ui thread:` line says what it was doing, and another says when it went on.
