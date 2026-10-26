# Boxes, the popup and the settings window

## Boxes

Up to four boxes go on the taskbar. Each box has its own services and limits, look (a preset or a
[custom box](custom-boxes.md)), background and place, and one service can be in more than one box.
`Show values as` (used / remaining), the warning colors, the popup's summary and dragging are one for all boxes.

A box is drawn 40 DIP high (inside the 48 DIP taskbar). For each service it shows an icon and the values and gauges of up
to two limits. Each service has a color of its own. When only the values change, the box keeps its width, so the apps on
the taskbar do not move.

- **Look:** five looks, `slim` (a thin bar, the default), `inline` (a bar beside the text), `stack` (a bar under the
  text), `pie` (a small pie) and `rings` (a ring for each limit), and custom boxes you make yourself.
- **Background:** one box (`One box`) or a card for each service (`Per service`).
- **Color:** each box has a background color and opacity of its own, the dark box by default; on a light color, or a
  nearly transparent one over a light taskbar, the values and labels turn dark and the service colors deepen, while the
  warning colors stay as they are.
- **Show values as:** the amount used (`Used`) or the amount left (`Remaining`). With remaining, the number is 100 minus
  the amount used, and the gauge is in the service's color for what is left and grey for what is used. PC's meters
  always show the amount used.
- **Warning colors:** a limit with 20% or less left turns orange, number and gauge, and one with 5% or less turns red
  with `!!` before the number, whichever way the values show. PC's meters turn red at 90% used. The popup's summary
  shows the same colors. They can be turned off on the settings window's `Boxes` page.
- **Icons:** the logos are their companies' trademarks, so they are not distributed with LimitPeeker. Put an `.svg` or
  `.png` named after the service (`gpt`, `claude`, `gemini`, `grok`) in `%LOCALAPPDATA%\LimitPeeker\icons`, beside the
  settings, so updating LimitPeeker or moving its folder keeps them; `Open icons folder` on the settings window's
  `Boxes` page opens it. Logos left in an `icons` folder next to the EXE, where 0.1.0 read them, are moved there when
  LimitPeeker starts. A one-color icon is painted in the service's color; without a file, a dot in the service's color is drawn. `PC` is a
  chip the program draws itself. A file put in, changed or taken away shows in the boxes within a second, and in the
  popup the next time it opens.

## Places

A box goes in one of four places on the taskbar. Choose it under `Place` in the popup or `Boxes > Place` in the settings
window, where they are listed in their order on the taskbar, from the left.

- `Left`: the left end of the taskbar. With the taskbar aligned left, the Start button and the apps move right by the box.
- `Start`: just left of the Start button. With the taskbar centered, the box and the apps are centered together. With it
  aligned left, this is the same spot as `Left`.
- `Apps`: just right of the last app. As apps are added the box follows them, and the apps overflow before the box.
- `Tray`: just before the tray, at the right end. This is where a box starts.
- `Hide` (last in the popup; `Hidden` in the settings window): takes the box off the taskbar and gives back its room,
  keeping its services, look and place. Its tab turns faint. Press a place to show it there again. A service only in
  hidden boxes is not read unless the popup's summary shows it. With every box hidden, only the tray icon and the popup
  are left.

More about places:

- With the Widgets (weather) button on and the taskbar centered, the button keeps the far left. A `Left` box stands just
  after it and follows it as the weather shows or hides its text; a `Start` box stays just before the Start button.
- With the taskbar aligned left, Windows puts the Widgets button just before the tray. An `Apps` box stands right beside
  the apps while there is room, and moves beyond the Widgets button when there are too many apps. Moving over lays the
  taskbar out once again.
- Boxes in one place stand side by side. In the popup, the line under the places says the chosen one in full; ← and → at
  the right of that line, or under `Boxes > Place` in the settings window, move the chosen box one step.
- Drag a box with the left mouse button to another place on the taskbar, or to the left or right of another box, and it
  moves there and is saved. While dragging it snaps to a place it comes near, and dropped far from any place it goes to the
  nearest. Dragging can be turned off with `Drag boxes to move them` in the settings window.
- Once, while a box was in a place other than beside the tray, Explorer restarted at the moment the taskbar's alignment
  was changed. The error was in Explorer's own layout of its app buttons, and it did not happen again under the same
  conditions.

## Tray icon

While a box is on the taskbar, the tray icon is a dial, a handle for the popup. With no box on it, the icon shows the
tightest of the limits the popup's summary shows, as a number, or a full circle at 100. A badge at the bottom right tells of a problem: a red exclamation mark for a failed read, an orange arrow for
something a restart of Explorer fixes, and an orange exclamation mark for any other reason the box cannot show. The
popup says why: a failed read under its service, with what to do when there is something, such as signing in
([more](services.md#when-values-are-read)). Its tooltip says whether a box is on the taskbar and lists every limit of
the services the boxes show; this PC's meters are left out, as the boxes already show them.

## Popup

Click the tray icon or right-click a box to open it. It opens with the boxes' settings folded: under the summary only the
boxes' tabs show, with `Edit` at their end, and `Show values as` under them. Right-clicking a box opens it with that box
chosen, so `Edit` unfolds that box. It follows the taskbar's light or dark mode and the accent color.

- **Summary.** The top shows, for each chosen service, the values of all its limits and the time until each resets. The
  filter line at the top of the summary (the funnel) has each service's icon and name; click one to turn it on or off in
  the summary. Services that are on are in bold; those that are off are faded. At first it matches the boxes; a service in
  no box can be in the summary alone, and it is read too.
- **Boxes.** At the bottom the boxes stand like tabs, named after the services they hold, and the card under the chosen
  tab holds that box's settings: `Style` (the five looks as one row of segments, custom boxes as chips under it, and `Customize…` to open the
  editor), `Place`,
  `Background`, `Services` (up to four, with icons and names like the summary's filter) and `Limits` (up to two for each
  service, four for PC). The swatch at the end of the `Background` line opens Windows' color dialog for the box's
  color, and the eyedropper beside it takes a color from anywhere on the screen (Esc cancels); the opacity is set in the
  settings window. Under the card, or the tabs while folded, `Show values as` chooses `Used` or `Remaining` for
  every box, the summary and the notifications; it always shows.
  - Folded, press `Edit`, or a box's tab to choose it, to unfold them; `Done` at the end of the tabs folds them again.
  - The `+` at the end of the tabs adds a new box modeled on the first, beside the tray; the `×` on the chosen tab removes
    that box (one always stays).
  - Double-click a tab, or press F2 with a tab focused, to rename the box in place. Enter or a click elsewhere saves it;
    Esc cancels. Names take up to 32 characters; left empty, the box goes back to the name made from what it holds.
- **Order.** Drag the services and limits that are on to change their order in the box. Drag the lines of the summary to
  change the summary's own order. With a chip that is on focused, Ctrl and the arrow keys also move it one step.
- **Buttons.** `Refresh now`, `Settings…` and `Exit`. `Refresh now` and the chips leave the popup open; `Settings…`,
  `Exit` and `Restart Explorer` (shown when restarting Explorer would bring the boxes back) close it.
- **Keyboard.** Tab and the arrow keys move around, and Enter or Space presses. A click outside, Esc, or another click on
  the tray icon closes it. Screen readers cannot read the popup yet, so choose the same options in the settings window.

## Settings window

Open it with `Settings…` in the popup. Like the Windows 11 Settings app, it has its pages on the left, and it opens on
the first, `General`. It needs the .NET Desktop Runtime 10; without the runtime, `Settings…` says so and offers to open
its download page. Without `LimitPeeker.Settings.exe` next to the EXE, `Settings…` in the popup is greyed out.

- `General`: the window's language, `Refresh now`, `Tool updates` (the tools'
  [own update commands](services.md#updates-of-the-tools), off at first), [start with Windows](startup.md),
  `Install folder` (where LimitPeeker's folder is, and `Open folder`), how to uninstall, the version and
  `Check for updates`.
- `Boxes`: the box tabs and the chosen box's name, place and order, look, background, color and opacity (`Default` puts
  both back) and services. Add and remove boxes
  with `Add box` and `Remove`, and rename them under `Name`. Services on in the box are cards with their limits and order
  buttons; turn the others on under `Add a service`. With a custom box, its blocks choose the services, and `Open editor`
  beside the note opens the editor. `Open icons folder`, under `Service logos`, opens the icons folder. Under
  `All boxes` at the bottom are
  `Show values as` and the switches for the warning colors and for dragging.
- `Custom boxes`: `Open editor` opens the box editor, and `Open folder` under `Box files` opens the folder of the boxes'
  files.
- `Popup`: the services the popup's summary shows.
- `Notifications`: which [notifications](#notifications) are sent, the share that is almost used up, and a test.

The settings window and the box editor are in English, Spanish, French, Japanese, Korean or Portuguese (Brazil).
`Language` on the `General` page is `Windows default` at first: the language Windows is displayed in when it is one of
these, and English otherwise. Picking another language opens the
window again in it, after asking about changes not saved in the editor. The boxes, the popup and the notifications stay
in English. To add a language, see [Translating the settings window](translating.md).

## Notifications

LimitPeeker sends Windows notifications about the limits of the services it reads (those in a box or in the popup's
summary), every limit of them, model limits included. Each is sent once for a limit until the limit resets:

- **Almost used up**: a limit reaches 90% used (or 80% or 95%, as chosen), with how long until it resets, as in
  `Claude 5h: 90% used`. With `Show values as` at `Remaining` it says what is left, as in `Claude 5h: 10% left`, and the
  choices on the `Notifications` page say it too: `20% left`, `10% left`, `5% left`.
- **Used up**: a limit reaches 100%, with when it resets, as in `Codex 5h limit reached` and `Resets at 15:40, in 2h 5m.`
  A limit that goes straight to 100% gets only this one.
- **Available again**: a used-up limit resets. A used-up limit is read 30 seconds after its reset time, so this comes then,
  not at the next read.
- **Reading keeps failing**: a service can't be read for 30 minutes, with what to do, as the popup says it, such as
  `Not signed in to Codex` and `Run codex login, or sign in to the Codex app`.

All four are on at first. The values found when LimitPeeker starts are taken as they are, so starting it again does not
repeat a notification; a limit already used up then is still told when it is available again. Selecting a notification
opens the popup. Windows shows them as toasts and keeps them in its notification center; its own settings (System >
Notifications) and Do not disturb decide whether they show. `Send a test notification` on the `Notifications` page sends
one at once to check that. This PC's meters and the mock figures send none.

## Saving

What you choose applies at once and is saved in `%LOCALAPPDATA%\LimitPeeker\settings.txt`, so it carries over to the next
run. Options given on the [command line](command-line.md) win over what is saved. Each box is saved as a `[box]` block.
