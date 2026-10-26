# Uninstall

**To remove LimitPeeker cleanly, run `uninstall.exe` in its folder before deleting the folder.** It does the same as
`limitpeeker.exe --remove`, and needs `limitpeeker.exe` next to it; without it, it says so and removes nothing. Deleting
only the folder leaves the things below on this PC.

It shows what it will remove and asks first, then closes LimitPeeker if it is running and removes:

- the Start with Windows entry (including the mark that Windows turned it off)
- `%LOCALAPPDATA%\LimitPeeker`: the settings, the custom boxes, the home folders of the hidden Claude Code, Antigravity
  CLI and Grok CLI, and the logs of what they handed over and of their updates
- the trust entries for the hidden Claude Code's folder in Claude Code's `.claude.json` (in `%USERPROFILE%`, or in
  `CLAUDE_CONFIG_DIR` when that is set): the folder's path, written both ways. They are there only if Claude's fallback
  ran. Only those entries go, with what the hidden Claude Code wrote in them; the rest stays as it is.
- the subscription of the DLL resting in the taskbar's Explorer, waiting to be started again. The DLL itself stays until
  Explorer ends.

At the end it shows what was removed and what was not. The folder with the EXE (and the log) stays, so delete it yourself
afterwards.

- If LimitPeeker does not close within 30 seconds, it says so and removes nothing.
- A log kept in the folder `LIMITPEEKER_LOG_DIR` names is not removed; delete it yourself.

- If the boxes have been on the taskbar, Explorer may hold `limitpeeker_xaml.dll` and the folder may not delete. Restart
  Windows Explorer in Task Manager, or sign out, and then delete it.
- What Codex, Claude Code, Antigravity CLI and Grok CLI leave in their own folders belongs to those tools, so it is not
  removed.
- A task you made yourself in Task Scheduler is not removed either. If one is left, the last window names it (one in Task
  Scheduler's `Microsoft` folder is not looked for); delete it in Task Scheduler.
