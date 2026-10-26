# Start with Windows

Turn on `Start with Windows` on the `General` page of the settings window, and LimitPeeker starts without arguments (the
usual run, attached to the taskbar) when you sign in to Windows.

- It only adds the EXE's path to your own startup list (`LimitPeeker` under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), so no administrator rights are needed.
- It also shows under Windows Settings > Apps > Startup and in Task Manager's Startup apps, and you can turn it off there.
  The settings window then says Windows turned it off, and turning it on again there turns it back on.
- After moving the folder, start the `limitpeeker.exe` in the new place once and the entry follows the new path. It does
  not change while the EXE in the old place is still there.
- Windows starts the programs in the startup list a little after you sign in. On a PC that starts many programs at sign-in
  this can take a minute or two.

## Start right at sign-in: a task of your own in Task Scheduler

To start right after signing in, make an "At log on" task in Task Scheduler yourself. LimitPeeker does not make this task,
but when there is one it finds it (not in Task Scheduler's `Microsoft` folder, which it does not look in) and shows it
under `Start with Windows` in the settings window and at the top of the log.

1. Open **Task Scheduler** from the Start menu, and choose **Create Task…** on the right (not "Create Basic Task").
2. **General** tab
   - Name: `LimitPeeker`
   - Choose **Run only when user is logged on**. With "Run whether user is logged on or not" it starts where nothing can
     be seen, with neither the tray icon nor the boxes.
   - Leave **Run with highest privileges** off. Started as administrator, the boxes do not attach.
3. **Triggers** tab > New: set Begin the task to **At log on** and Settings to **Specific user** (your account), then OK. No
   delay is needed: if the taskbar is not there yet, LimitPeeker waits for it and then attaches.
4. **Actions** tab > New: choose **Start a program**, and put the full path of `limitpeeker.exe` in Program/script (for
   example `C:\Tools\LimitPeeker\limitpeeker.exe`). Leave the arguments empty.
5. **Conditions** tab: turn off **Start the task only if the computer is on AC power**. Left on, it does not start while a
   laptop runs on battery.
6. **Settings** tab: turn off **Stop the task if it runs longer than**. With the default of 3 days, LimitPeeker is closed
   after that.
7. Press OK. If it asks for administrator rights or refuses the task, you cannot make one yourself on this PC; use the
   startup list instead.
8. Back in the settings window, the task shows under `Start with Windows`. If the startup list is on too, it says one of the
   two is enough, so turn `Start with Windows` off. Leaving both on does no harm, as whichever starts second ends at once.

When you move the folder, this task does not follow. If the settings window shows it as "another copy", fix the program
path in the task.
