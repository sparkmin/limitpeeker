using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Text.Json;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;

namespace LimitPeeker.Settings;

/// <summary>
/// An item in a list: what is sent, and what is shown. Its title is also its
/// name for screen readers, which take an item's text.
/// </summary>
internal sealed record Option(string Key, string Title)
{
    public override string ToString() => Title;
}

/// <summary>
/// The popup's choices in a window of their own. Each change goes to the
/// controller at once, and what shows is always the state the controller sent
/// back, so the window and the popup never disagree.
/// </summary>
public partial class MainWindow : Window
{
    readonly nint controller;
    readonly DispatcherTimer watch = new() { Interval = TimeSpan.FromSeconds(1) };
    // Counts Refresh now down to when the controller takes it again.
    readonly DispatcherTimer resting = new() { Interval = TimeSpan.FromMilliseconds(250) };
    // When, in Unix milliseconds, that is; null never, with the mock figures.
    long? refreshAt = 0;
    readonly Dictionary<string, CheckBox> services = [];
    readonly Dictionary<(string, string), CheckBox> limits = [];
    /// <summary>The popup summary's check boxes, by service.</summary>
    readonly Dictionary<string, CheckBox> summary = [];
    /// <summary>The boxes' names and the one chosen, as their tabs were last built.</summary>
    string boxNames = "";
    readonly Dictionary<string, (Button Up, Button Down)> moves = [];
    // The Fluent icons, for the buttons that move a service.
    static readonly FontFamily Icons = new("Segoe Fluent Icons, Segoe MDL2 Assets");
    nint self;
    // True while the controller's state is being shown, so showing it is not
    // taken for a change.
    bool showing;
    // Whether the controller has answered with its state. Until it has, and
    // after a command did not reach it, this window greets it again each
    // second.
    bool greeted;
    // The services and limits the cards were built for; they are built again
    // only when these change, and otherwise only checked and enabled.
    string cards = "";
    string looks = "";
    // How the boxes show values, which the almost-used-up list was said in.
    string notifyFill = "used";
    // This version and the repository Check for updates asks, as the
    // controller sent them.
    string version = "";
    string repository = "";
    // How long Check for updates waits for GitHub's answer.
    const int UpdateSeconds = 10;
    // The box's color every look starts with, shown while the state says
    // default; the chosen box's color as last sent, and whether it is that
    // default. Must match style::BOX_COLOR.
    internal const string DefaultColor = "#FF1F2227";
    uint boxColor = 0xFF1F2227;
    bool boxColorDefault = true;
    // Whether the eyedropper has the pointer, and whether the opacity's
    // thumb is being dragged, which sends once it is let go.
    bool picking;
    bool draggingOpacity;
    // The box editor, a window of its own, and the state it is kept up with;
    // started for the editor alone, this window stays hidden.
    EditorWindow? editor;
    State? lastState;
    internal readonly bool EditorOnly;
    bool closed;
    // Set once this window has started itself again in another language, so
    // closing does not ask about the editor's changes a second time.
    bool reopening;

    public MainWindow(nint controller, bool editorOnly = false, string? page = null)
    {
        this.controller = controller;
        EditorOnly = editorOnly;
        InitializeComponent();
        // Japanese in a Japanese font, not Malgun Gothic's forms.
        if (Loc.Font is { } font)
        {
            FontFamily = new FontFamily(font);
        }
        // The page asked for, as when the window opens again in another
        // language, or else the first.
        Nav.SelectedItem = Nav.Items.OfType<ListBoxItem>()
            .FirstOrDefault(item => string.Equals(item.Tag as string, $"{page}Page", StringComparison.OrdinalIgnoreCase))
            ?? Nav.Items[0];
        // Windows' language, then each language in its own name.
        LanguageBox.ItemsSource = Loc.Languages
            .Select(code => new Option(code, Loc.NameOf(code)))
            .Prepend(new Option("", Loc.Plain("general.language.windows")))
            .ToList();
        FillBox.ItemsSource = new[] { new Option("used", Loc.Plain("boxes.values.used")), new Option("left", Loc.Plain("boxes.values.left")) };
        BackgroundBox.ItemsSource = new[]
        {
            new Option("box", Loc.Plain("boxes.background.box")),
            new Option("cards", Loc.Plain("boxes.background.cards")),
        };
        NotifyAtBox.ItemsSource = NotifyAtOptions(notifyFill);
        // Where a box goes, as on the taskbar from the left.
        PlaceBox.ItemsSource = new[]
        {
            new Option("left", Loc.Plain("boxes.place.left")),
            new Option("before-start", Loc.Plain("boxes.place.before_start")),
            new Option("after-apps", Loc.Plain("boxes.place.after_apps")),
            new Option("tray", Loc.Plain("boxes.place.tray")),
            // Kept, but not on the taskbar.
            new Option("hidden", Loc.Plain("boxes.place.hidden")),
        };
        resting.Tick += (_, _) => ShowRefresh();
        // The opacity goes once the thumb is let go, not at every step of
        // the drag.
        OpacitySlider.AddHandler(Thumb.DragStartedEvent, new DragStartedEventHandler((_, _) => draggingOpacity = true));
        OpacitySlider.AddHandler(Thumb.DragCompletedEvent, new DragCompletedEventHandler((_, _) =>
        {
            draggingOpacity = false;
            SendOpacity();
        }));
        // Closed with the editor open, its changes are asked about first, and
        // a No keeps both. With the controller gone they could not be saved,
        // so nothing is asked.
        Closing += (_, e) => e.Cancel = !reopening && editor is not null && Link.IsWindow(controller) && !editor.MayDrop();
        // The window goes when the controller does, and greets it until it
        // answers.
        watch.Tick += (_, _) =>
        {
            if (!Link.IsWindow(controller))
            {
                Close();
            }
            else if (!greeted)
            {
                Link.Send(controller, self, Link.Hello);
            }
        };
        // Back in front, a sign-in task made meanwhile in Task Scheduler is
        // looked for.
        Activated += (_, _) =>
        {
            if (greeted)
            {
                Send(Link.StartupCheck);
            }
        };
    }

    protected override void OnSourceInitialized(EventArgs e)
    {
        base.OnSourceInitialized(e);
        self = new WindowInteropHelper(this).Handle;
        Frame.WithoutMaximize(self);
        HwndSource.FromHwnd(self)?.AddHook(Receive);
        Body.IsEnabled = false;
        if (controller == 0 || !Link.IsWindow(controller))
        {
            Status.Text = Loc.T("window.status.alone");
            return;
        }
        Status.Text = Loc.T("window.status.waiting");
        Link.Send(controller, self, Link.Hello);
        watch.Start();
    }

    nint Receive(nint window, int message, nint from, nint data, ref bool handled)
    {
        if (message != Link.WmCopyData)
        {
            return 0;
        }
        if (Link.Read(data, Link.State) is { } text)
        {
            greeted = true;
            // Shown after the controller's call returns, which waits for this.
            Dispatcher.BeginInvoke(() => Render(State.Parse(text)));
            handled = true;
            return 1;
        }
        // The editor's: a box to open, a draft's preview, and what a save or
        // delete came to.
        if (Link.Read(data, Link.Box) is { } box)
        {
            Dispatcher.BeginInvoke(() =>
            {
                Editor().Open(box);
                ShowEditor();
            });
            handled = true;
            return 1;
        }
        if (Link.Read(data, Link.Preview) is { } preview)
        {
            Dispatcher.BeginInvoke(() => editor?.ShowPreview(preview));
            handled = true;
            return 1;
        }
        if (Link.Read(data, Link.Result) is { } result)
        {
            Dispatcher.BeginInvoke(() => editor?.Answered(result));
            handled = true;
            return 1;
        }
        if (Link.Read(data, Link.Show) is not null)
        {
            // Hidden while it was started for the editor alone.
            if (!IsVisible)
            {
                Show();
            }
            if (WindowState == WindowState.Minimized)
            {
                WindowState = WindowState.Normal;
            }
            Activate();
            handled = true;
            return 1;
        }
        return 0;
    }

    void Render(State state)
    {
        showing = true;
        try
        {
            Status.Text = Loc.T("window.status.ready");
            Body.IsEnabled = true;
            var names = string.Join("\t", state.Looks);
            if (names != looks)
            {
                looks = names;
                StyleBox.ItemsSource = state.Looks
                    .Select(look => new Option(look, Titled(look)))
                    .ToList();
            }
            StyleBox.SelectedValue = state.Look;
            // The boxes as tabs, the one chosen, its name unless it is being
            // typed, where it goes and whether it can move among the boxes
            // there; and dragging, for all.
            var boxes = $"{state.Current}\t{string.Join("\t", state.Tiles)}\t{string.Concat(state.HiddenTiles.Select(h => h ? 1 : 0))}";
            if (boxes != boxNames)
            {
                boxNames = boxes;
                BuildTabs(state);
            }
            AddButton.IsEnabled = state.Tiles.Count < 4;
            RemoveButton.IsEnabled = state.Tiles.Count > 1;
            if (!NameBox.IsKeyboardFocusWithin)
            {
                NameBox.Text = Named(state);
            }
            PlaceBox.SelectedValue = state.Place;
            EarlierButton.IsEnabled = state.CanMoveEarlier;
            LaterButton.IsEnabled = state.CanMoveLater;
            DragCheck.IsChecked = state.Drag;
            WarnCheck.IsChecked = state.Warn;
            // Each notification on or off, and the line of almost used up,
            // which matters only while that one is on.
            foreach (var check in new[] { NotifyNearCheck, NotifyFullCheck, NotifyBackCheck, NotifyFailingCheck })
            {
                check.IsChecked = state.Notify.GetValueOrDefault((string)check.Tag, true);
            }
            if (state.Fill != notifyFill)
            {
                notifyFill = state.Fill;
                NotifyAtBox.ItemsSource = NotifyAtOptions(notifyFill);
            }
            NotifyAtBox.SelectedValue = state.NotifyAt;
            NotifyAtBox.IsEnabled = NotifyNearCheck.IsChecked == true;
            FillBox.SelectedValue = state.Fill;
            BackgroundBox.SelectedValue = state.Background;
            if (!picking)
            {
                ShowColor(state.Color);
            }
            LanguageBox.SelectedValue = state.Language;
            UpdatesCheck.IsChecked = state.Updates;
            // A custom box's blocks choose its services and limits, and it is
            // drawn as one box.
            var custom = state.Look.StartsWith("box:", StringComparison.Ordinal);
            BackgroundBox.IsEnabled = !custom;
            ServicesPanel.IsEnabled = !custom;
            EditorLinkButton.Visibility = custom ? Visibility.Visible : Visibility.Collapsed;
            ServicesNote.Text = custom
                ? Loc.T("boxes.services.custom")
                : Loc.T("boxes.services.caption", ("services", state.MostServices), ("limits", state.MostLimits));
            // Which limits show changes which have move buttons.
            var built = string.Join("\n", state.Services.Select(s =>
                $"{s.Id}\t{s.Name}\t{s.Shown}\t{string.Join("\t", s.Limits.Select(l => $"{l.Label}={l.Shown}"))}"));
            if (built != cards)
            {
                cards = built;
                Build(state);
            }
            Check(state);
            refreshAt = state.RefreshAt;
            resting.Start();
            ShowRefresh();
            lastState = state;
            editor?.Known(state);
            // Started for the editor alone, it shows once the state is in.
            if (EditorOnly && editor is null)
            {
                ShowEditor();
            }
        }
        finally
        {
            showing = false;
        }
    }

    /// <summary>The boxes as tabs, the chosen one checked; a tab chooses its box.</summary>
    void BuildTabs(State state)
    {
        BoxTabs.Children.Clear();
        for (var index = 0; index < state.Tiles.Count; index++)
        {
            var at = index;
            var tab = new RadioButton
            {
                Content = state.Tiles[index],
                Style = (Style)FindResource("Tab"),
                GroupName = "Boxes",
                IsChecked = index == state.Current,
                // A hidden box's tab is faint.
                Opacity = state.HiddenTiles.ElementAtOrDefault(index) ? 0.5 : 1,
            };
            AutomationProperties.SetName(tab, state.Tiles[index]);
            tab.Click += (_, _) => Send($"select\t{at}");
            BoxTabs.Children.Add(tab);
        }
    }

    /// <summary>The chosen box's name as its tab shows it.</summary>
    static string Named(State state) => state.Tiles.ElementAtOrDefault(state.Current) ?? "";

    /// <summary>
    /// Gives the chosen box the name typed, when it changed, as the popup's tab
    /// does; empty, it is named by what it shows.
    /// </summary>
    void Rename()
    {
        if (lastState is { } state && NameBox.Text.Trim() != Named(state))
        {
            Send($"name\t{state.Current}\t{NameBox.Text}");
        }
    }

    void NameLeft(object sender, KeyboardFocusChangedEventArgs e) => Rename();

    /// <summary>Enter keeps the name typed, and Esc puts back the one it had.</summary>
    void NameKey(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            Rename();
            e.Handled = true;
        }
        else if (e.Key == Key.Escape && lastState is { } state)
        {
            NameBox.Text = Named(state);
            e.Handled = true;
        }
    }

    /// <summary>Shows the page picked on the left, from its top.</summary>
    void NavChanged(object sender, SelectionChangedEventArgs e)
    {
        if (Nav.SelectedItem is not ListBoxItem { Tag: string picked } item)
        {
            return;
        }
        foreach (var page in new[] { BoxesPage, PopupPage, CustomPage, NotificationsPage, GeneralPage })
        {
            page.Visibility = page.Name == picked ? Visibility.Visible : Visibility.Collapsed;
        }
        PageTitle.Text = AutomationProperties.GetName(item);
        Scroller.ScrollToTop();
    }

    /// <summary>
    /// A card per service in the box, in its order, each with its limits and
    /// buttons that move it; the others small, in a grid, to add one.
    /// </summary>
    void Build(State state)
    {
        ServicesPanel.Children.Clear();
        services.Clear();
        limits.Clear();
        moves.Clear();
        foreach (var service in state.Services.Where(s => s.Shown))
        {
            var panel = new StackPanel();
            var head = new DockPanel();
            var box = new CheckBox { Content = service.Name, VerticalAlignment = VerticalAlignment.Center };
            box.Click += (_, _) => Send($"service\t{service.Id}");
            services[service.Id] = box;
            if (service.Shown)
            {
                var up = Move("\uE70E", Loc.Plain("boxes.services.up"));
                up.Click += (_, _) => Send($"move\t{service.Id}\tup");
                var down = Move("\uE70D", Loc.Plain("boxes.services.down"));
                down.Click += (_, _) => Send($"move\t{service.Id}\tdown");
                moves[service.Id] = (up, down);
                var buttons = new StackPanel { Orientation = Orientation.Horizontal };
                buttons.Children.Add(up);
                buttons.Children.Add(down);
                DockPanel.SetDock(buttons, Dock.Right);
                head.Children.Add(buttons);
            }
            head.Children.Add(box);
            panel.Children.Add(head);
            if (service.Shown)
            {
                var row = new WrapPanel { Margin = new Thickness(28, 4, 0, 0) };
                if (service.Limits.Count == 0)
                {
                    row.Children.Add(new TextBlock { Text = Loc.T("boxes.services.unread"), Style = (Style)FindResource("Caption") });
                }
                // The shown limits come first, in the box's order; with more
                // than one, each moves a place earlier or later.
                var shownCount = service.Limits.Count(l => l.Shown);
                var at = 0;
                foreach (var limit in service.Limits)
                {
                    var check = new CheckBox { Content = limit.Label, Margin = new Thickness(0, 0, 8, 0) };
                    check.Click += (_, _) => Send($"limit\t{service.Id}\t{limit.Label}");
                    limits[(service.Id, limit.Label)] = check;
                    var item = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 0, 16, 0) };
                    item.Children.Add(check);
                    if (limit.Shown && shownCount > 1)
                    {
                        var place = at;
                        var earlier = Move("\uE76B", Loc.Plain("boxes.services.earlier"));
                        earlier.IsEnabled = place > 0;
                        earlier.Click += (_, _) => Send($"move-limit\t{service.Id}\t{limit.Label}\t{place - 1}");
                        var later = Move("\uE76C", Loc.Plain("boxes.services.later"));
                        later.IsEnabled = place < shownCount - 1;
                        later.Click += (_, _) => Send($"move-limit\t{service.Id}\t{limit.Label}\t{place + 1}");
                        item.Children.Add(earlier);
                        item.Children.Add(later);
                    }
                    if (limit.Shown)
                    {
                        at++;
                    }
                    row.Children.Add(item);
                }
                panel.Children.Add(row);
            }
            ServicesPanel.Children.Add(new Border { Style = (Style)FindResource("Card"), Child = panel });
        }
        var others = state.Services.Where(s => !s.Shown).ToList();
        if (others.Count > 0)
        {
            var grid = new UniformGrid { Columns = 3 };
            foreach (var service in others)
            {
                var box = new CheckBox { Content = service.Name, Margin = new Thickness(0, 2, 16, 2) };
                box.Click += (_, _) => Send($"service\t{service.Id}");
                services[service.Id] = box;
                grid.Children.Add(box);
            }
            var add = new StackPanel();
            add.Children.Add(new TextBlock { Text = Loc.T("boxes.services.add"), Margin = new Thickness(0, 0, 0, 8) });
            add.Children.Add(grid);
            ServicesPanel.Children.Add(new Border { Style = (Style)FindResource("Card"), Child = add });
        }
        // Any offered service can be in the summary, on the taskbar or not.
        SummaryPanel.Children.Clear();
        summary.Clear();
        foreach (var service in state.Services)
        {
            var box = new CheckBox { Content = service.Name, Margin = new Thickness(0, 0, 16, 0) };
            box.Click += (_, _) => Send($"summary-service\t{service.Id}");
            summary[service.Id] = box;
            SummaryPanel.Children.Add(box);
        }
    }

    /// <summary>The editor, made when first wanted; closed, it is made again.</summary>
    EditorWindow Editor()
    {
        if (editor is null)
        {
            editor = new EditorWindow(Send);
            editor.Closed += (_, _) =>
            {
                editor = null;
                // Started for the editor alone, the program goes with it.
                if (!IsVisible && !closed)
                {
                    Close();
                }
            };
        }
        return editor;
    }

    /// <summary>Shows the editor in front, kept up with the state.</summary>
    void ShowEditor()
    {
        var shown = Editor();
        if (!shown.IsVisible)
        {
            shown.Show();
        }
        if (shown.WindowState == WindowState.Minimized)
        {
            shown.WindowState = WindowState.Normal;
        }
        shown.Activate();
        if (lastState is { } state)
        {
            shown.Known(state);
        }
    }

    void EditorClicked(object sender, RoutedEventArgs e) => ShowEditor();

    /// <summary>
    /// Opens the icons folder beside the programs in Explorer, made first when
    /// it is not there; why not, under the card.
    /// </summary>
    void IconsClicked(object sender, RoutedEventArgs e)
    {
        var folder = Path.Combine(AppContext.BaseDirectory, "icons");
        try
        {
            Directory.CreateDirectory(folder);
            Process.Start(new ProcessStartInfo(folder) { UseShellExecute = true });
            LogosNote.Visibility = Visibility.Collapsed;
        }
        catch (Exception ex) when (ex is Win32Exception or IOException or UnauthorizedAccessException)
        {
            LogosNote.Text = Loc.T("boxes.logos.failed", ("why", ex.Message));
            LogosNote.Visibility = Visibility.Visible;
        }
    }

    protected override void OnClosed(EventArgs e)
    {
        closed = true;
        if (editor is { } open)
        {
            open.Leaving = true;
            open.Close();
        }
        base.OnClosed(e);
    }

    /// <summary>A small button with a Fluent icon.</summary>
    static Button Move(string glyph, string tip) => new()
    {
        Content = new TextBlock { Text = glyph, FontFamily = Icons, FontSize = 12 },
        ToolTip = tip,
        Padding = new Thickness(8, 4, 8, 4),
        Margin = new Thickness(4, 0, 0, 0),
    };

    /// <summary>
    /// Checks what shows, and greys what the rules hold: no more on once the
    /// most are, never the last one off, and no move past either end. The
    /// controller checks them again.
    /// </summary>
    void Check(State state)
    {
        var on = state.Services.Count(s => s.Shown);
        var order = state.Services.Where(s => s.Shown).Select(s => s.Id).ToList();
        foreach (var (id, (up, down)) in moves)
        {
            var at = order.IndexOf(id);
            up.IsEnabled = at > 0;
            down.IsEnabled = at >= 0 && at < order.Count - 1;
        }
        foreach (var service in state.Services)
        {
            if (services.TryGetValue(service.Id, out var box))
            {
                box.IsChecked = service.Shown;
                box.IsEnabled = service.Shown ? on > 1 : on < state.MostServices;
            }
            var shown = service.Limits.Count(l => l.Shown);
            foreach (var limit in service.Limits)
            {
                if (limits.TryGetValue((service.Id, limit.Label), out var check))
                {
                    check.IsChecked = limit.Shown;
                    check.IsEnabled = limit.Shown ? shown > 1 : shown < state.MostLimitsOf(service.Id);
                }
            }
        }
        // The summary has no most, and never none.
        foreach (var (id, box) in summary)
        {
            var inSummary = state.Summary.Contains(id);
            box.IsChecked = inSummary;
            box.IsEnabled = !inSummary || state.Summary.Count > 1;
        }
        // Turned off in Windows' Startup apps, it is off, and says where.
        StartupCheck.IsChecked = state.Startup == "on";
        StartupNote.Text = Loc.T(state.Startup == "blocked" ? "general.startup.blocked" : "general.startup.caption");
        TaskNote.Text = Tasks(state);
        TaskNote.Visibility = TaskNote.Text.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
        // The version, and Check for updates once there is a repository to
        // ask.
        version = state.Version;
        repository = state.Repository;
        VersionText.Text = version.Length > 0 ? $"LimitPeeker {version}" : "LimitPeeker";
        UpdateButton.Visibility = repository.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    /// <summary>
    /// What the Task Scheduler tasks a person made to start it at sign-in
    /// come to: what each starts, and what is off about it, each a sentence
    /// of its own so a language words it whole.
    /// </summary>
    static string Tasks(State state)
    {
        var lines = state.Tasks.Select(task =>
        {
            var sentences = new List<string>
            {
                task.ThisExe
                    ? Loc.T("general.task.this", ("task", task.Path))
                    : Loc.T("general.task.other", ("task", task.Path), ("program", task.Program)),
            };
            if (!task.Enabled)
            {
                sentences.Add(Loc.T("general.task.off"));
            }
            sentences.AddRange(task.Problem switch
            {
                "admin" => [Loc.T("general.task.admin")],
                "hidden" => [Loc.T("general.task.hidden")],
                _ => [],
            });
            return string.Join(" ", sentences);
        }).ToList();
        if (state.Startup == "on" && state.Tasks.Any(t => t.Enabled && t.ThisExe && t.Problem == ""))
        {
            lines.Add(Loc.T("general.task.both"));
        }
        return string.Join("\n", lines);
    }

    /// <summary>Turns the start at sign-in on or off; the controller keeps it in the registry.</summary>
    void StartupClicked(object sender, RoutedEventArgs e) =>
        Send($"startup\t{(StartupCheck.IsChecked == true ? "on" : "off")}");

    /// <summary>
    /// Has every service read at once. The controller sends back when it takes
    /// the next request, and the button counts down to it.
    /// </summary>
    void RefreshClicked(object sender, RoutedEventArgs e) => Send(Link.Refresh);

    /// <summary>
    /// Asks GitHub once, now, for the repository's newest release and says
    /// whether it is newer than this version. Nothing is downloaded; the
    /// releases page opens only when asked. This is the only time LimitPeeker
    /// goes on the network itself.
    /// </summary>
    async void CheckUpdatesClicked(object sender, RoutedEventArgs e)
    {
        if (repository.Length == 0) return;
        UpdateButton.IsEnabled = false;
        ReleasesButton.Visibility = Visibility.Collapsed;
        UpdateSays(Loc.T("general.version.checking"));
        try
        {
            using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(UpdateSeconds) };
            client.DefaultRequestHeaders.UserAgent.ParseAdd($"LimitPeeker/{(version.Length > 0 ? version : "0")}");
            client.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
            var answer = await client.GetStringAsync($"https://api.github.com/repos/{repository}/releases/latest");
            using var json = JsonDocument.Parse(answer);
            var newest = (json.RootElement.GetProperty("tag_name").GetString() ?? "").TrimStart('v', 'V');
            if (Newer(newest, version))
            {
                UpdateSays(Loc.T("general.version.newer", ("version", newest)));
                ReleasesButton.Visibility = Visibility.Visible;
            }
            else
            {
                UpdateSays(Loc.T("general.version.latest"));
            }
        }
        catch (HttpRequestException ex) when (ex.StatusCode == HttpStatusCode.NotFound)
        {
            UpdateSays(Loc.T("general.version.no_release"));
        }
        catch (TaskCanceledException)
        {
            UpdateSays(Loc.T("general.version.no_answer", ("seconds", UpdateSeconds)));
        }
        catch (Exception ex) when (ex is HttpRequestException or JsonException or KeyNotFoundException or InvalidOperationException)
        {
            UpdateSays(Loc.T("general.version.failed", ("why", ex.Message)));
        }
        finally
        {
            UpdateButton.IsEnabled = true;
        }
    }

    void UpdateSays(string text)
    {
        UpdateNote.Text = text;
        UpdateNote.Visibility = Visibility.Visible;
    }

    void ReleasesClicked(object sender, RoutedEventArgs e)
    {
        try
        {
            Process.Start(new ProcessStartInfo($"https://github.com/{repository}/releases/latest") { UseShellExecute = true });
        }
        catch (Win32Exception ex)
        {
            UpdateSays(Loc.T("general.version.releases_failed", ("why", ex.Message)));
        }
    }

    /// <summary>
    /// Whether <paramref name="newest"/> is a later version than
    /// <paramref name="current"/>, number by number. A part after a hyphen
    /// is left out, and what cannot be read is not newer.
    /// </summary>
    internal static bool Newer(string newest, string current)
    {
        static int[]? Numbers(string text)
        {
            var parts = text.Split('-', 2)[0].Split('.');
            var numbers = new int[parts.Length];
            for (var i = 0; i < parts.Length; i++)
            {
                if (!int.TryParse(parts[i], out numbers[i])) return null;
            }
            return numbers;
        }
        if (Numbers(newest) is not { } a || Numbers(current) is not { } b) return false;
        for (var i = 0; i < Math.Max(a.Length, b.Length); i++)
        {
            var (x, y) = (i < a.Length ? a[i] : 0, i < b.Length ? b[i] : 0);
            if (x != y) return x > y;
        }
        return false;
    }

    /// <summary>
    /// The button as the controller's cooldown has it, wherever Refresh now
    /// was pressed, in the popup or here: greyed with the seconds left, so a
    /// greyed button is not taken for a fault, and for good with the mock
    /// figures.
    /// </summary>
    void ShowRefresh()
    {
        var left = refreshAt is { } at
            ? (int)Math.Ceiling((at - DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()) / 1000.0)
            : 0;
        RefreshButton.Content = left > 0
            ? Loc.Plain("general.refresh.resting", ("seconds", left))
            : Loc.Plain("general.refresh.button");
        RefreshButton.IsEnabled = refreshAt is not null && left <= 0;
        if (left <= 0)
        {
            resting.Stop();
        }
    }

    /// <summary>
    /// Shows the box's color: the swatch, the opacity, and Default greyed
    /// while it is the default.
    /// </summary>
    void ShowColor(string color)
    {
        boxColorDefault = color == "default";
        boxColor = (boxColorDefault ? null : ParseColor(color)) ?? ParseColor(DefaultColor) ?? 0;
        Swatch.Background = new SolidColorBrush(ToColor(boxColor));
        OpacitySlider.Value = Math.Round((boxColor >> 24) * 100.0 / 255);
        OpacityText.Text = $"{(int)Math.Round(OpacitySlider.Value)}%";
        DefaultColorButton.IsEnabled = !boxColorDefault;
    }

    /// <summary>A color as #AARRGGBB, and one read from that.</summary>
    internal static string Hex(uint argb) => $"#{argb:X8}";
    internal static uint? ParseColor(string text) =>
        text.Length == 9 && text[0] == '#' && uint.TryParse(text[1..], System.Globalization.NumberStyles.HexNumber, null, out var argb)
            ? argb
            : null;
    internal static Color ToColor(uint argb) =>
        Color.FromArgb((byte)(argb >> 24), (byte)(argb >> 16), (byte)(argb >> 8), (byte)argb);

    /// <summary>
    /// The opacity a color picked keeps: the box's, or opaque when the box
    /// has no background, so the pick shows.
    /// </summary>
    uint KeptAlpha => boxColor >> 24 == 0 ? 0xFF000000u : boxColor & 0xFF000000u;

    void SendColor(uint argb) => Send($"color\t{Hex(argb)}");

    /// <summary>Windows' color dialog over this window, started at the box's color.</summary>
    void ColorClicked(object sender, RoutedEventArgs e)
    {
        if (Picker.Choose(self, boxColor) is { } rgb)
        {
            SendColor(KeptAlpha | rgb & 0x00FFFFFF);
        }
    }

    /// <summary>
    /// Takes the pointer to pick a color from anywhere on the screen: the
    /// swatch shows what is under it until a click takes that; Esc, the
    /// right button or losing the pointer gives up.
    /// </summary>
    void DropperClicked(object sender, RoutedEventArgs e)
    {
        picking = true;
        Mouse.OverrideCursor = Cursors.Cross;
        Keyboard.Focus(DropperButton);
        Mouse.Capture(this, CaptureMode.Element);
    }

    uint? PixelUnder(MouseEventArgs e)
    {
        var at = PointToScreen(e.GetPosition(this));
        return Picker.PixelAt((int)Math.Round(at.X), (int)Math.Round(at.Y));
    }

    void EndPicking(uint? rgb)
    {
        if (!picking)
        {
            return;
        }
        picking = false;
        Mouse.OverrideCursor = null;
        if (Mouse.Captured == this)
        {
            ReleaseMouseCapture();
        }
        if (rgb is { } picked)
        {
            SendColor(KeptAlpha | picked & 0x00FFFFFF);
        }
        else if (lastState is { } state)
        {
            ShowColor(state.Color);
        }
    }

    protected override void OnPreviewMouseMove(MouseEventArgs e)
    {
        if (picking)
        {
            if (PixelUnder(e) is { } rgb)
            {
                Swatch.Background = new SolidColorBrush(ToColor(KeptAlpha | rgb & 0x00FFFFFF));
            }
            e.Handled = true;
            return;
        }
        base.OnPreviewMouseMove(e);
    }

    protected override void OnPreviewMouseDown(MouseButtonEventArgs e)
    {
        if (picking)
        {
            EndPicking(e.ChangedButton == MouseButton.Left ? PixelUnder(e) : null);
            e.Handled = true;
            return;
        }
        base.OnPreviewMouseDown(e);
    }

    protected override void OnPreviewKeyDown(KeyEventArgs e)
    {
        if (picking)
        {
            if (e.Key == Key.Escape)
            {
                EndPicking(null);
            }
            e.Handled = true;
            return;
        }
        base.OnPreviewKeyDown(e);
    }

    protected override void OnLostMouseCapture(MouseEventArgs e)
    {
        EndPicking(null);
        base.OnLostMouseCapture(e);
    }

    /// <summary>The color and the opacity back to the default.</summary>
    void DefaultColorClicked(object sender, RoutedEventArgs e) => Send("color\tdefault");

    /// <summary>
    /// The opacity, 0 to 100, on the box's color; sent as it changes from the
    /// keyboard or a click on the track, and once a drag of the thumb ends.
    /// </summary>
    void OpacityChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        OpacityText.Text = $"{(int)Math.Round(OpacitySlider.Value)}%";
        if (!showing && !draggingOpacity)
        {
            SendOpacity();
        }
    }

    void SendOpacity()
    {
        var alpha = (uint)Math.Round(OpacitySlider.Value * 255 / 100);
        var argb = alpha << 24 | boxColor & 0x00FFFFFF;
        if (argb != boxColor)
        {
            SendColor(argb);
        }
    }

    void StyleChanged(object sender, SelectionChangedEventArgs e) => Pick("look", StyleBox);
    void FillChanged(object sender, SelectionChangedEventArgs e) => Pick("fill", FillBox);
    void BackgroundChanged(object sender, SelectionChangedEventArgs e) => Pick("background", BackgroundBox);
    void PlaceChanged(object sender, SelectionChangedEventArgs e) => Pick("place", PlaceBox);

    /// <summary>
    /// Adds a box, removes the one chosen, or moves it left or right among the
    /// boxes in its place.
    /// </summary>
    void AddClicked(object sender, RoutedEventArgs e) => Send("add");
    void RemoveClicked(object sender, RoutedEventArgs e) => Send("remove");
    void EarlierClicked(object sender, RoutedEventArgs e) => Send("order\tearlier");
    void LaterClicked(object sender, RoutedEventArgs e) => Send("order\tlater");

    /// <summary>
    /// Lets boxes be dragged on the taskbar, or not, however the switch was
    /// turned, UI Automation's toggle included; not while the state is shown.
    /// </summary>
    void DragChanged(object sender, RoutedEventArgs e)
    {
        if (!showing)
        {
            Send($"drag\t{(DragCheck.IsChecked == true ? "on" : "off")}");
        }
    }

    /// <summary>
    /// Lets low values warn in color, or not, however the switch was turned;
    /// not while the state is shown.
    /// </summary>
    void WarnChanged(object sender, RoutedEventArgs e)
    {
        if (!showing)
        {
            Send($"warn\t{(WarnCheck.IsChecked == true ? "on" : "off")}");
        }
    }

    /// <summary>
    /// Runs each tool's own update command every 15 minutes, or not, however
    /// the switch was turned; not while the state is shown.
    /// </summary>
    void UpdatesChanged(object sender, RoutedEventArgs e)
    {
        if (!showing)
        {
            Send($"updates\t{(UpdatesCheck.IsChecked == true ? "on" : "off")}");
        }
    }

    /// <summary>
    /// A notification turned on or off, the kind from its switch's tag;
    /// the line of almost used up; and a test sent now.
    /// </summary>
    void NotifyChanged(object sender, RoutedEventArgs e)
    {
        if (!showing && sender is CheckBox { Tag: string kind } check)
        {
            Send($"notify\t{kind}\t{(check.IsChecked == true ? "on" : "off")}");
        }
    }
    void NotifyAtChanged(object sender, SelectionChangedEventArgs e) => Pick("notify-at", NotifyAtBox);
    void NotifyTestClicked(object sender, RoutedEventArgs e) => Send("notify-test");

    /// <summary>
    /// Another language picked: the editor's changes not saved are asked
    /// about first, and a No keeps the language. Then the controller keeps
    /// it, and the window opens again in it.
    /// </summary>
    void LanguageChanged(object sender, SelectionChangedEventArgs e)
    {
        if (showing || LanguageBox.SelectedValue is not string code || lastState is not { } state || code == state.Language)
        {
            return;
        }
        if (editor is not null && !editor.MayDrop())
        {
            showing = true;
            LanguageBox.SelectedValue = state.Language;
            showing = false;
            return;
        }
        Send($"language\t{code}");
        Reopen(code);
    }

    /// <summary>
    /// Starts this window again with the same controller, in `language` and
    /// on the General page, and closes this one: the words are taken once,
    /// as the window starts, not changed in place.
    /// </summary>
    void Reopen(string language)
    {
        var start = new ProcessStartInfo(Environment.ProcessPath ?? "") { UseShellExecute = false };
        foreach (var argument in Environment.GetCommandLineArgs().Skip(1).Where(argument =>
            !argument.StartsWith("--language=", StringComparison.Ordinal)
            && !argument.StartsWith("--page=", StringComparison.Ordinal)
            && argument != "--editor"))
        {
            start.ArgumentList.Add(argument);
        }
        start.ArgumentList.Add($"--language={language}");
        start.ArgumentList.Add("--page=general");
        try
        {
            Process.Start(start);
        }
        catch (Win32Exception)
        {
            // Kept, it is in that language the next time the window opens.
            return;
        }
        reopening = true;
        Close();
    }

    /// <summary>
    /// The shares that may be almost used up, said as the boxes show values:
    /// used, or what is left. Each is kept as the share used, so changing how
    /// values show keeps the line.
    /// </summary>
    static List<Option> NotifyAtOptions(string fill) => new[] { 80, 90, 95 }
        .Select(at => new Option($"{at}", fill == "left"
            ? Loc.Plain("notifications.near.left", ("share", 100 - at))
            : Loc.Plain("notifications.near.share", ("share", at))))
        .ToList();

    void Pick(string what, ComboBox box)
    {
        if (!showing && box.SelectedValue is string key)
        {
            Send($"{what}\t{key}");
        }
    }

    void Send(string command)
    {
        if (!Link.Send(controller, self, command))
        {
            // Greeted again, the controller sends its state, which puts the
            // boxes back as they are.
            Status.Text = Loc.T("window.status.no_answer");
            greeted = false;
        }
    }

    /// <summary>A look's name as the popup shows it; a custom box's as it is named.</summary>
    static string Titled(string name) => name.StartsWith("box:", StringComparison.Ordinal)
        ? name["box:".Length..]
        : name.Length == 0 ? name : char.ToUpperInvariant(name[0]) + name[1..];
}
