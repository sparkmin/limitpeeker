# Services and how they are read

LimitPeeker never signs in to anything. It starts the AI tools you already have, signed in, and asks them for their usage
limits, the same way for every read. It never reads or stores sign-in data.

| Service | Read from | Limits |
| --- | --- | --- |
| Codex (ChatGPT subscription) | The Codex app, CLI or VS Code extension | `5h`, `7d` |
| Claude | Claude Code: the CLI or the VS Code extension | `5h`, `7d` and per-model weekly limits |
| Antigravity (Gemini models) | The Antigravity CLI (`agy`) | `7d`, `5h` on Pro and Ultra, and `1d` when Antigravity sets a daily limit |
| Grok | The Grok CLI (`grok`) | The current billing period (`7d`, `1mo`, or its length in days) |
| PC | This PC itself | CPU, RAM, GPU, VRAM |

## Codex

LimitPeeker looks for what typing `codex` runs first: in each folder on the PATH, `codex.exe`, or the `codex.cmd` that
`npm install -g @openai/codex` makes. That file is only read, to find the program npm installed inside `node_modules`
(`@openai/codex-win32-x64\vendor\…\codex.exe`), which is started itself, so Node is not needed. Then it looks in the
Codex app (`%LOCALAPPDATA%\OpenAI\Codex\bin\*`), where the Codex installer puts it
(`%LOCALAPPDATA%\Programs\OpenAI\Codex\bin` and `%USERPROFILE%\.codex\packages\standalone\current\bin`), npm's own
folder (`%APPDATA%\npm`), and the extension for VS Code or VS Code Insiders (`openai.chatgpt-*`). The app, the CLI and
the extension all carry the same program. It starts `codex app-server` without a window, asks for the usage limits
(`account/rateLimits/read`) and ends it at once. A limit's name comes from the length of its period (5 hours `5h`, 1
week `7d`).

## Claude

LimitPeeker looks for what typing `claude` runs first: in each folder on the PATH, `claude.exe`, or the `claude.cmd`
that `npm install -g @anthropic-ai/claude-code` makes, which is only read to find the program npm installed
(`@anthropic-ai/claude-code-win32-x64\claude.exe`). Then it looks where Claude Code's installer puts it
(`%USERPROFILE%\.local\bin`), in npm's own folder (`%APPDATA%\npm`), and inside the Claude Code extension for VS Code or
VS Code Insiders, which carries the same program. An npm install made without its install script has only a placeholder,
not the program, and is passed over. The Claude desktop app is a different program and is not used.

While Claude shows, it starts Claude Code without a window in `%LOCALAPPDATA%\LimitPeeker\claude-home`, asks for the same
data as `/usage` (`get_usage`) and ends it at once. No model is called, so it costs no tokens, and no session history is
kept. The 5-hour (`5h`), weekly (`7d`) and per-model weekly limits (for example `Fable 7d`) come back; the two shown in a
box are chosen under `Limits` in the popup or the settings window.

`get_usage` is a feature Claude Code calls experimental, so when it fails, LimitPeeker opens `/usage` in a hidden console
and reads only `5h` and `7d`. When Claude Code answers that no Claude plan is signed in (signed out, or on an API key), or
that the plan has no limits, `/usage` would say the same, so it is not opened.

That fallback is the only time LimitPeeker writes to Claude Code's own file: it marks
`%LOCALAPPDATA%\LimitPeeker\claude-home` as a trusted folder in `.claude.json` (in `%USERPROFILE%`, or in
`CLAUDE_CONFIG_DIR` when that is set), under both spellings of the path, so the hidden Claude Code does not stop to ask.
LimitPeeker writes nothing else in that file; the hidden Claude Code adds its own notes under those entries, as it
does for any folder it runs in. LimitPeeker also writes the hidden Claude Code's settings,
`%LOCALAPPDATA%\LimitPeeker\claude-settings.json`, which set its status line.

## Antigravity (Gemini)

The service is named `Antigravity` in the boxes, the popup and the settings window, and its icon file is `gemini`.
While it shows, LimitPeeker starts the Antigravity CLI (`agy.exe`, on the PATH or in `%LOCALAPPDATA%\agy\bin`) without a
window, takes the limits handed to its status line, and ends it with Ctrl+C. A read takes about 8 seconds. No prompt is
sent, so no limit is used. A Google AI subscription has separate limits for each product; the ones shown here are
Antigravity's limits for Gemini models. Free accounts have only a weekly limit (`7d`); Pro and Ultra also have a 5-hour one
(`5h`). A daily limit (`1d`) shows when Antigravity hands one. Google's Gemini CLI is a different tool and is not used.

- `agy` starts in a home folder of LimitPeeker's own, `%LOCALAPPDATA%\LimitPeeker\agy-home`. The status line is set only in
  that folder's settings, so nothing is left in the settings of the `agy` you use. The sign-in is the one `agy` keeps in
  your account.
- The limit for Claude and GPT models inside Antigravity (`3p-weekly`) is not a Gemini limit, so it is not shown, only
  logged.
- `GEMINI_API_KEY` and `GOOGLE_API_KEY` are not passed to the hidden `agy`, because with a key no limits come back.

## Grok

While Grok shows, LimitPeeker starts the Grok CLI (`grok.exe`, on the PATH or in `%USERPROFILE%\.grok\bin`) without a window
as `grok agent stdio`, asks for the billing data (`_x.ai/billing`, or `x.ai/billing` when the CLI does not know the first) and ends it at
once. No prompt is sent. The current period
(`7d` when weekly, `1mo` when monthly) becomes one limit, and the end of the period is when it resets. On a paid (SuperGrok)
account the share of credits used is the value, the same as the CLI's `/usage` shows; the CLI leaves it out while nothing is
used, so a paid account shows `0%` then. A paid account's week starts when it was bought. A free account gets no share, so
it shows `--` and the time left in the period.

The CLI LimitPeeker starts has a home folder of its own, `%LOCALAPPDATA%\LimitPeeker\grok-home`, so its logs and caches
stay out of your `~/.grok`. It signs in with your own sign-in file there (or where `GROK_AUTH_PATH` or `GROK_HOME`
point), which only the CLI reads, and it does not update itself during a read (see [Updates of the
tools](#updates-of-the-tools)). These settings are given only to the CLI LimitPeeker starts; the Grok you start yourself
is not changed. The first read unpacks about 15 MB of the CLI's own files into that folder, and its logs there are
cleared once they pass 1 MB. If a later CLI no longer signs in that way, LimitPeeker reads in your `~/.grok` as before,
for the rest of that run, and says so in the log. Grok is read every 15 minutes.

## PC

This PC's CPU, RAM, GPU and VRAM load. At first it is in no box. Turn it on under a box's `Services` like any other
service, and choose what shows under `Limits` (CPU and RAM at first, up to four). With more than two shown, `slim` puts
them on one line and the other looks in two columns of two. You can also make a box with only PC in it, and use it in the
blocks of a custom box.

- It is measured every second only while it shows in a box, or while the popup is open and it is in the summary. Only what
  shows is measured; when it shows nowhere, nothing is.
- The values mean what they mean in Task Manager. CPU is the `% Processor Utility` performance counter; RAM is the share
  in use, not available; GPU is the busiest engine; VRAM is the share of the GPU's dedicated memory in use. They are taken
  at a different moment, so they may differ a little from Task Manager.
- With more than one GPU, the one with the most dedicated memory is measured. A built-in GPU with less than 512 MB of its
  own shares system memory, so its VRAM is Task Manager's shared GPU memory (the amount used over the amount it may share,
  often half the RAM). The popup says `VRAM: shared GPU memory` on the right of the PC title line.
- In the popup, the RAM and VRAM lines show the amount used and the total on the right, as in `12.9/15.8 GB`. The box on the
  taskbar shows only the %.
- It is always the amount used. Even with `Remaining` chosen it shows the share used, and rings start at twelve o'clock.
  At 90% or more it turns red with `!!`.
- It is not part of the tray icon's gauge.
- While it is on, the controller uses about 2 ms of CPU a second (about 3–5 ms with the GPU shown). The box inside
  Explorer moves every second and uses about 4–5 ms a second more.

## When values are read

- Values are read again every 5 minutes (Grok every 15). After a failure it tries again, waiting from 1 minute up to 15
  minutes. `Refresh now` reads every service shown at once, and can be used again after 10 seconds; a service whose
  update is running is read after it.
- Right after you sign in to Windows, the first reads wait until about 90 seconds after the sign-in, as Windows is still
  busy then; until then the values show `--`. `Refresh now` reads at once.
- A service is read only while it is in some box or in the popup's summary, as each read starts a program. Added, it is
  read at once.
- A tool that is not found is looked for again every 15 minutes.
- While a tool's update runs (see below), that service is not read; the next read comes after it.
- A limit not read yet shows `--`. While reads fail, the last value read stays, faded; if nothing was ever read, it shows
  `?`.
- When a read fails, the popup says under the service what went wrong and when it is tried again, and, when there is
  something to do, what: for example `Not signed in to Codex` and `Run codex login, or sign in to the Codex app`, or
  `OpenAI did not answer (503)`, which needs nothing. The tool's own words are in the [log](command-line.md#log).
- A limit with 20% or less left is drawn in orange, number and gauge, and one with 5% or less in red, with `!!` before
  the number ([warning colors](boxes-and-popup.md)).
- When none of the four tools is found, no box appears and the tray says `No service connected`. `PC` can still be chosen
  and shown.

## Updates of the tools

The hidden runs that read the limits do not update the tools: a run is ended after a few seconds, which could cut a
download short, and the Antigravity CLI's own update check opens a terminal window. Instead, with `Tool updates` turned
on, LimitPeeker runs that tool's own update command every 15 minutes while the service is read, just after a read:
`claude update`, `agy update`, `grok update` and `codex update`. Each runs without a window, in the same folder and
environment as the reads, and downloads from that tool's maker, so a tool that is only read through LimitPeeker stays up to date too.
The Grok CLI's runs in your own Grok home, not in the one its reads use: it installs an update into the home it runs
in.

- `Tool updates` is on the settings window's `General` page and is off at first. Off, the tools are updated only when
  you update them yourself, and the log says so once for each tool. Turned off while an update runs, that update is
  left to finish.
- It is put off while you are using that tool, that is, while a program runs from the same file outside LimitPeeker, and
  tried again after the next read. So it does not update at the same time as a tool you started.
- It is not stopped. If it is still running after 5 minutes, or LimitPeeker closes, it is left to finish.
- The versions before and after, or why it failed, go to the [log](command-line.md#log). What the tool says goes to
  `%LOCALAPPDATA%\LimitPeeker\<tool>-update.log` (`claude`, `agy`, `grok` or `codex`), written anew each time, and when it last ran
  to `<tool>-updated.timestamp` beside it, so a restart does not run it again at once. When an update fails, what it said
  is also copied to `<tool>-update-failed.log`, kept until the next failure.
- A version that did not change is logged as `version unchanged`, whatever the tool said: Codex's installer says
  `Update ran successfully!` even when it was already the newest.
- Codex is updated only when it is the one Codex's installer put in, under `%USERPROFILE%\.codex\packages\standalone\releases`,
  which the installer's folder on the PATH leads to: `codex update` runs the installer again. A Codex installed with npm
  is left to npm (`npm install -g @openai/codex`), as LimitPeeker starts its program without npm's script, which tells
  Codex how it was installed.
- The Claude Code and Codex inside the VS Code extension are not updated here either: VS Code updates them with the
  extension.
- Desktop apps, such as the Claude and Codex apps, keep their own copies and update them themselves.
