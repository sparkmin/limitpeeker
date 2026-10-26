# How it attaches to the taskbar

LimitPeeker is a controller (`limitpeeker.exe`) and a small DLL that runs inside Explorer (`limitpeeker_xaml.dll`). The
controller puts the DLL into Explorer with XAML diagnostics, and the two talk through shared memory. This is not a way of
extending the taskbar that Windows supports.

- **Taking a spot.** Beside the tray, it widens the left padding inside the tray panel (the first row panel inside `SystemTray.SystemTrayFrame`) by the
  boxes' width. After the last app, it widens the right margin of the taskbar's row of buttons (`TaskbarFrameRepeater`).
  At the left end and before the Start button, it widens the left margin of the Start button, so the row itself stays
  at the screen's left: while Search is open, Windows draws its search box where the taskbar's one is inside that row.
  With the taskbar centered, the Widgets button keeps the far left, and a box at the left end stands just after it.
  Windows lays out the apps, folds the search box, shrinks the icons and overflows them to fit on its own, so however
  many apps there are, a box is not covered. Nothing is added inside Explorer's elements: adding a child there crashed
  Explorer when the overflow changed.
- **Drawing.** A box is a Border of its own, drawn over that spot. Its width is measured with the real XAML font at the
  widest form (for example `100%`), and measured again when the scale or the format changes. When it must grow, the
  padding widens first, and the box changes once the apps have kept clear for 0.5 seconds, or after 3 seconds in any
  case.
- **The controller decides what is drawn.** It sends the text, colors, looks and blocks, and the DLL draws what it gets.
  So changing a look never needs a new DLL or an Explorer restart. Icons too are drawn by the controller in pixels for the
  scale and sent over, so no file is opened inside Explorer.
- **Putting things back.** The original padding and margins are kept and put back on exit. When another program changes
  them, it steps back rather than fight. When the taskbar is no longer horizontal at the bottom of the main display, it
  gives them back and hides, and takes the spot again when it returns.
- **When the controller stops.** The controller keeps renewing a 1.5-second lease. When the renewals stop, the DLL takes
  the boxes off and puts everything back on its own. If the controller had only stalled for a moment, it takes the
  boxes back as soon as it runs again.
- **The DLL stays until Explorer ends**, to avoid an unsafe unload. On exit only the boxes come off, and the DLL rests,
  holding its subscription, until Explorer ends. Started again meanwhile, LimitPeeker does not attach anew; that DLL brings
  the boxes back. `--remove` releases the resting DLL's subscription. Started after that, it reuses the same Explorer's
  session and attaches anew. A session that ended in an error is not reused.
- **When the DLL does not start.** Attaching returns before the DLL inside Explorer starts. When the DLL has not taken
  the session 10 seconds later, as happened once right after signing in, LimitPeeker attaches again, up to three times
  for one Explorer; then the popup says it could not attach. Each attach is numbered, and a DLL loaded by an earlier
  attach that starts late is turned down, so there is never a second set of boxes. The log says whether the DLL came
  into Explorer and why it turned the session down.
- **When Explorer restarts**, the new Explorer is recognized by its process number and start time, and LimitPeeker
  attaches again. LimitPeeker never restarts Explorer by itself; `Restart Explorer` in the popup works only when you press
  it.
- **Snap layouts.** Attaching to an Explorer that has a Snap layouts window thread (`SnapFlyoutHost Thread`) ended Explorer
  (`Windows.UI.Xaml.dll`, `0x328f23`). This is a defect on the Windows side: on attaching, Windows' XAML diagnostics walks
  every XAML thread and stops on this one. Hovering a window's maximize button makes Explorer create this thread and keep
  it until it ends, so while it is there the controller waits rather than attach, and the tray says
  `Restart Explorer to show the box`. Started again in an Explorer it has attached to before, it does not attach anew, so
  it skips this wait.

## Known limits

- A Windows update may change how it works or affect Explorer. It was checked on one PC, on Windows 11 builds 26200 and
  26300.
- To attach to an Explorer with a Snap layouts thread for the first time, Explorer must be restarted. The defect is on the
  Windows side, and no way around it has been found.
- Only a horizontal taskbar at the bottom of the main display is supported. Showing only on the main display with two
  displays, and the taskbar's auto-hide, were checked once.
- Checked at scales of 100%, 125%, 150% and 175%. The text in the boxes does not follow Windows' text size setting.
- Screen readers cannot read the popup. The same options can be chosen in the settings window.
- The longest run so far is about 41 hours, without a crash. No tests guarantee the absence of flicker, of overlap with
  the Start menu's animation, or of leaks over long runs.
