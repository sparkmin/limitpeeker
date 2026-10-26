using System.Globalization;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Data;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using Shapes = System.Windows.Shapes;

namespace LimitPeeker.Settings;

/// <summary>What the stage draws the box in: its width inside, height, padding and corners in hundredths of a DIP, and its colors.</summary>
internal sealed record BoxFrame(int Width, int Height, int Padding, int Radius, uint Color, uint Track);

/// <summary>One item of a scene as the controller laid it out, with the block it comes from.</summary>
internal sealed record SceneItem(int Block, int Kind, int X, int Y, int W, int H, int Fill, uint Color, uint Track,
    int Size, int Weight, int Options, int Slot, string Text);

/// <summary>A block as the list shows it: what it is, and where; the index keeps equal texts apart.</summary>
internal sealed record BlockRow(int Index, string Title, string Detail)
{
    public override string ToString() => Detail == "" ? Title : $"{Title} · {Detail}";
}

/// <summary>A style a preset is made in: its key in a box's file, its name, what it is, and a small picture.</summary>
internal sealed record BoxStyle(string Key, string Title, string About, Geometry Icon);

/// <summary>A preset a new one may start from, in its group: the samples or one's own. Its name is also its name for screen readers.</summary>
internal sealed record StartRow(string Name, string Group)
{
    public override string ToString() => Name;
}

/// <summary>
/// A shelf of the presets, by its word: one's own, the basic samples, or the samples of the preset looks for the
/// first box or for PC; in this order.
/// </summary>
internal sealed record Shelf(string Key, string Title, int Order)
{
    static readonly string[] Keys = ["mine", "basic", "ai", "pc"];

    public static Shelf Of(string key)
    {
        var at = Array.IndexOf(Keys, key);
        var title = key switch
        {
            "basic" => Loc.Plain("editor.presets.basic"),
            "ai" => Loc.Plain("editor.presets.ai"),
            "pc" => Loc.Plain("editor.presets.pc"),
            _ => Loc.Plain("editor.presets.mine"),
        };
        return new Shelf(at < 0 ? "mine" : key, title, Math.Max(at, 0));
    }

    /// <summary>What a screen reader says of its head: its name.</summary>
    public string Spoken => Title;
    public override string ToString() => Title;
}

/// <summary>A preset as the browser shows it; the key is "" for a new one not saved yet.</summary>
internal sealed record PresetRow(string Key, string Name, BoxStyle Style, bool Sample, bool InUse, bool Unsaved, string ShelfKey)
{
    public Shelf Group => Shelf.Of(ShelfKey);
    public Geometry Icon => Style.Icon;
    public string Detail => Unsaved ? Loc.Plain("editor.presets.unsaved", ("style", Style.Title))
        : InUse ? Loc.Plain("editor.presets.in_use", ("style", Style.Title))
        : Sample ? Loc.Plain("editor.presets.sample", ("style", Style.Title))
        : Style.Title;
    public Visibility LockShown => Sample ? Visibility.Visible : Visibility.Collapsed;
    public Visibility UseShown => InUse ? Visibility.Visible : Visibility.Collapsed;
    public override string ToString() => Name;
}

/// <summary>
/// The custom box editor, a window of its own. At its left the presets, the
/// samples and the user's own; a new one starts from its style's sample.
/// Each change goes to the controller as a draft; the controller, which alone
/// knows a box's rules and lays it out as the taskbar will, sends back the
/// scene the stage draws. Saving, deleting and putting a preset on the
/// taskbar are the controller's too. In a free box blocks are picked, moved,
/// sized, aligned and grouped with the mouse, as on a slide, and kept inside
/// the box; the numbers are for fine adjustment. A preset's style is chosen
/// when it is made. A group is sized and ordered as one, and a block of it
/// is picked alone by clicking it again.
/// </summary>
public partial class EditorWindow : Window
{
    static readonly Option[] Kinds =
    [
        new("icon", Loc.Plain("editor.kind.icon")), new("ring", Loc.Plain("editor.kind.ring")),
        new("rings", Loc.Plain("editor.kind.rings")), new("pie", Loc.Plain("editor.kind.pie")),
        new("bar", Loc.Plain("editor.kind.bar")), new("text", Loc.Plain("editor.kind.text")),
        new("space", Loc.Plain("editor.kind.space")),
    ];
    static readonly Option[] Shows =
    [
        new("value", Loc.Plain("editor.shows.value")), new("label", Loc.Plain("editor.shows.label")),
        new("both", Loc.Plain("editor.shows.both")), new("resets", Loc.Plain("editor.shows.resets")),
        new("name", Loc.Plain("editor.shows.name")),
        new("own", Loc.Plain("editor.shows.own")),
    ];
    static readonly Option[] Weights =
    [
        new("400", Loc.Plain("editor.weight.regular")), new("500", Loc.Plain("editor.weight.medium")),
        new("600", Loc.Plain("editor.weight.semibold")), new("700", Loc.Plain("editor.weight.bold")),
    ];
    static readonly Option[] Aligns =
    [
        new("start", Loc.Plain("editor.align.start")), new("center", Loc.Plain("editor.align.center")),
        new("end", Loc.Plain("editor.align.end")),
    ];
    // The styles a preset is made in, the first the one a new preset starts
    // in. Another style is another card to pick.
    static readonly BoxStyle[] Styles =
    [
        new("columns", Loc.Plain("editor.style.columns"), Loc.T("editor.style.columns.about"),
            Picture("M1,4 h5 v12 h-5 Z M8,4 h5 v5 h-5 Z M8,11 h5 v5 h-5 Z M15,4 h4 v12 h-4 Z")),
        new("free", Loc.Plain("editor.style.free"), Loc.T("editor.style.free.about"),
            Picture("M1,3 h8 v7 h-8 Z M11,6 h8 v4 h-8 Z M4,12 h11 v5 h-11 Z")),
    ];
    // The palette: eight colors light, as they are and dark, then greys.
    static readonly string[] Hues =
    [
        "#FF9A9A", "#FFC08A", "#FFE28A", "#A8E6A1", "#8EE3D6", "#9ED8FF", "#B9B6FF", "#F0A8E8",
        "#FF5A5A", "#FF8C3A", "#FFC83D", "#5FCB5A", "#2EC4B0", "#4CC2FF", "#7C79FF", "#E06CD2",
        "#C62828", "#D2601A", "#C99400", "#2E8B35", "#13867A", "#1C7FC0", "#4B47C9", "#A63A9B",
    ];
    static readonly string[] Greys = ["#FFFFFF", "#E0E0E0", "#BDBDBD", "#9E9E9E", "#757575", "#585858", "#3A3A3A", "#1E1E1E"];
    // The picked blocks' blue, the marks' white, and the guides' and sizes' grey.
    static readonly Brush Accent = Frozen(Color.FromRgb(0x4C, 0xC2, 0xFF));
    static readonly Brush Mark = Frozen(Colors.White);
    static readonly Brush Guide = Frozen(Color.FromArgb(0x66, 0xFF, 0xFF, 0xFF));
    static readonly Brush Sizing = Frozen(Color.FromArgb(0xB3, 0xFF, 0xFF, 0xFF));
    static readonly Brush Pill = Frozen(Color.FromArgb(0xE0, 0x1C, 0x1C, 0x1E));
    // The taskbar's font, which the controller measures texts in.
    static readonly FontFamily SceneFont = new("Segoe UI Variable Text, Segoe UI");
    // Room around the box for its sizes, in pixels.
    const double Room = 64;
    // The widest box the taskbar takes, in DIPs.
    const double Widest = 600;
    // The zooms Ctrl and the wheel step through, each about a fifth past the
    // last, from the box's real size to eight times it; and the ones the zoom
    // box always lists, which are among them.
    static readonly double[] Zooms = [1, 1.25, 1.5, 1.75, 2, 2.5, 3, 3.5, 4, 5, 6, 7, 8];
    static readonly double[] ZoomPicks = [2, 3, 4];

    readonly Action<string> send;
    readonly DispatcherTimer drafting = new() { Interval = TimeSpan.FromMilliseconds(150) };
    // The box being edited, the name it was opened under ("" for a new one),
    // whether it is a sample, which cannot be changed, and whether it has
    // changes not saved.
    BoxModel? draft;
    string opened = "";
    bool sample;
    bool dirty;
    // True while the controls are being filled, so filling them is not taken
    // for a change.
    bool loading;
    bool asked;
    State? known;
    // The presets the controller has, the rows the browser shows, and the
    // name a new preset asked for is to have.
    List<BoxInfo> presets = [];
    string presetRows = "";
    string? pendingName;
    // The blocks picked, whole groups at a time, and the one the panel shows.
    readonly HashSet<int> chosen = [];
    int primary = -1;
    // The scene the controller last sent, and where each block of a free box
    // stood in the draft it was laid out from.
    BoxFrame? frame;
    readonly List<SceneItem> items = [];
    Dictionary<int, (string Id, uint Color)> icons = [];
    Dictionary<int, Point> sentPlaces = [];
    Dictionary<int, Point> shownPlaces = [];
    // True from a change that makes the scene's sizes another block's or
    // another kind's until the next scene; and the own width and height the
    // W and H boxes show for a block without its own.
    bool stale;
    string ownW = "";
    string ownH = "";
    // A gesture with the mouse on the stage, and the box's width when it
    // began, which it may grow.
    // Pan moves the view, not the blocks.
    enum Gesture { None, Move, Resize, Marquee, Pan }
    Gesture gesture;
    // Space held over the stage, which makes the left button move the view;
    // and where a move of the view began: the box's place in the view and
    // the pointer's.
    bool spaceHeld;
    Point panFrom;
    Point panAt;
    bool moved;
    Point pressAt;
    Dictionary<int, Point> startPlaces = [];
    // Blocks picked already that Shift or Ctrl and a click let go, unless the
    // press moves them.
    List<int>? letGo;
    Rect startRect;
    // What a resize started from: the block or group sized, and each of its
    // blocks' place, text size and thickness.
    List<int> resizing = [];
    Dictionary<int, (Rect Place, double? Size, double? Thickness)> startSizes = [];
    // The group entered, whose blocks are picked one at a time, and the block
    // a click that does not move enters it at.
    int? into;
    int? enterAt;
    double startWidth;
    int handle;
    Rect marquee;
    // A box just opened is drawn as large as the stage has room for.
    bool fitting;
    // Where the box's outer top-left corner is on the stage, in pixels: put
    // in the middle of what shows when a box opens, the zoom changes or the
    // stage is sized again, and otherwise kept, so a box that grows grows to
    // its right and never slides under the pointer.
    double left0 = Room;
    double top0 = Room;
    bool centering = true;
    // The stage's own size when it last put the box in the middle: scroll
    // bars that come and go as the box is zoomed change only what it shows,
    // and leave the box where the zoom put it.
    Size viewSize;
    // What the wheel turned short of a notch, as fine wheels and touchpads
    // send less than one at a time.
    int wheelRest;
    // The least the stage spans after a zoom scrolled it, in pixels, until
    // the box is put in the middle again: where the box ends short of what
    // shows, the scroll that keeps the point under the pointer would
    // otherwise be cut back at the stage's edge.
    double spanX;
    double spanY;
    // Each step back or forward: the box as its file, and the place of the
    // block the panel showed then, as blocks are known by their places.
    readonly Stack<(string Json, int Primary)> undo = new();
    readonly Stack<(string Json, int Primary)> redo = new();
    DateTime lastEdit;
    /// <summary>
    /// Set as the settings window goes, which takes the editor with it
    /// without asking again: it has asked, or the controller is gone and what
    /// is not saved could not be saved any more.
    /// </summary>
    internal bool Leaving;

    internal EditorWindow(Action<string> send)
    {
        this.send = send;
        InitializeComponent();
        WireBoxColor();
        // Japanese in a Japanese font, not Malgun Gothic's forms.
        if (Loc.Font is { } font)
        {
            FontFamily = new FontFamily(font);
        }
        KindBox.ItemsSource = Kinds;
        ShowBox.ItemsSource = Shows;
        WeightBox.ItemsSource = Weights;
        AlignBox.ItemsSource = Aligns;
        ShowZoom();
        BuildPalette();
        BuildStyleCards();
        drafting.Tick += (_, _) =>
        {
            drafting.Stop();
            SendDraft();
        };
        PreviewKeyDown += Keyed;
        // The box goes back to the middle once the stage shows more or less:
        // when the stage is told its new size, the room it shows is still the
        // old one. Only a stage sized again does, not scroll bars that come
        // and go.
        StageView.ScrollChanged += (_, e) =>
        {
            if ((e.ViewportWidthChange != 0 || e.ViewportHeightChange != 0) && StageView.RenderSize != viewSize)
            {
                viewSize = StageView.RenderSize;
                centering = true;
                DrawStage();
            }
        };
        StageView.PreviewMouseWheel += StageWheeled;
        StageView.PreviewMouseDown += ViewPressed;
        // The stage is only as large as the box and its room, so a press on
        // the view past it is one on the stage's empty place, which picks
        // several.
        StageView.MouseLeftButtonDown += (_, e) =>
        {
            if (e.OriginalSource is Visual source && source != Stage && !Stage.IsAncestorOf(source) && !OnScrollBar(source))
            {
                StagePressed(Stage, e);
            }
        };
        StageView.PreviewMouseMove += ViewMoved;
        StageView.PreviewMouseUp += (_, e) =>
        {
            if (gesture == Gesture.Pan)
            {
                e.Handled = true;
                StageView.ReleaseMouseCapture();
            }
        };
        StageView.LostMouseCapture += (_, _) =>
        {
            if (gesture == Gesture.Pan)
            {
                gesture = Gesture.None;
                ShowPanning();
            }
        };
        PreviewKeyUp += (_, e) =>
        {
            if (e.Key == Key.Space && spaceHeld)
            {
                e.Handled = true;
                LetGoOfSpace();
            }
        };
        // A key let go in another window never comes back here.
        Deactivated += (_, _) => LetGoOfSpace();
        SourceInitialized += (_, _) => HwndSource.FromHwnd(new WindowInteropHelper(this).Handle)?.AddHook(Hooked);
        Hint.On(ColorText, "#RRGGBB");
        Hint.On(SizeText, "11");
        Hint.On(ThicknessText, Loc.Plain("editor.block.thickness.hint"));
        Hint.On(OwnText, Loc.Plain("editor.block.words.hint"));
        HeightShown.Text = Loc.Plain("editor.box.height.fixed", ("height", Number(Tall)));
        // Closing with changes not saved asks first.
        Closing += (_, e) => e.Cancel = !Leaving && !MayDrop();
        ShowEditor();
    }

    /// <summary>How many pixels a DIP of the box takes on the stage: one of <see cref="Zooms"/>.</summary>
    double Zoom { get; set; } = 3;

    /// <summary>How high the box is, in DIPs: the taskbar's box, always 40.</summary>
    double Tall => (frame?.Height ?? 4000) / 100.0;

    /// <summary>The controller's state: the presets and the services. With no box open, the one shown now, or the first, is asked for.</summary>
    internal void Known(State state)
    {
        known = state;
        presets = state.Boxes;
        // The shelves folded as the controller keeps them; when they change, the shelves are laid out again.
        if (!folded.SetEquals(state.Folded))
        {
            folded = [.. state.Folded];
            presetRows = "";
        }
        if (!asked && draft is null)
        {
            asked = true;
            var name = state.Look.StartsWith("box:", StringComparison.Ordinal)
                ? state.Look["box:".Length..]
                : presets.FirstOrDefault()?.Name;
            if (name is not null)
            {
                send($"edit\t{name}");
            }
        }
        ListPresets();
        ShowUse();
        // The box's color, which the stage draws the preset on.
        KnownColor(state);
        if (draft is not null && !dirty)
        {
            FillBlock();
        }
    }

    /// <summary>
    /// A box from the controller: whether it is a sample, or "new" for a
    /// style's sample to start a new preset from, then its file's text.
    /// </summary>
    internal void Open(string text)
    {
        var (flag, json) = text.IndexOf('\n') is var at and >= 0 ? (text[..at], text[(at + 1)..]) : (text, "");
        BoxModel box;
        try
        {
            box = BoxModel.Parse(json);
        }
        catch (Exception error) when (error is FormatException or InvalidOperationException or System.Text.Json.JsonException)
        {
            BoxNote.Text = Loc.T("editor.note.unreadable", ("why", error.Message));
            return;
        }
        asked = true;
        centering = true;
        if (flag == "new")
        {
            box.Name = pendingName ?? Unique(Loc.Plain("editor.presets.first_name"));
            pendingName = null;
            Begin(box);
            return;
        }
        fitting = true;
        draft = box;
        opened = box.Name;
        sample = flag == "1";
        dirty = false;
        stale = true;
        undo.Clear();
        redo.Clear();
        StartUnchosen();
    }

    /// <summary>What a save or delete came to.</summary>
    internal void Answered(string text)
    {
        switch (text.Split('\t'))
        {
            case ["saved", var name]:
                var shown = known?.Look is { } look && (look == "box:" + name || (opened != "" && look == "box:" + opened));
                opened = name;
                sample = false;
                dirty = false;
                SaveNote.Text = Loc.T(shown ? "editor.save.shown" : "editor.save.saved");
                break;
            case ["deleted", _]:
                draft = null;
                opened = "";
                dirty = false;
                chosen.Clear();
                primary = -1;
                frame = null;
                items.Clear();
                SaveNote.Text = Loc.T("editor.save.deleted");
                break;
            case ["failed", var why]:
                SaveNote.Text = why;
                break;
        }
        ShowEditor();
    }

    /// <summary>Fills the editor from the box, and greys what cannot be changed.</summary>
    void ShowEditor()
    {
        loading = true;
        try
        {
            var editable = draft is not null && !sample;
            var free = draft?.Free == true;
            DuplicateButton.IsEnabled = draft is not null;
            DeleteButton.IsEnabled = draft is not null && !sample && opened != "";
            BoxName.IsReadOnly = !editable;
            BoxName.IsEnabled = draft is not null;
            SampleBadge.Visibility = sample && draft is not null ? Visibility.Visible : Visibility.Collapsed;
            AddButton.IsEnabled = editable;
            ArrangeTools.IsEnabled = editable && free;
            ArrangeTools.ToolTip = free ? null : Loc.Plain("editor.toolbar.free_only.tip");
            BlockPanel.IsEnabled = editable;
            SaveButton.IsEnabled = editable && (dirty || opened == "");
            UndoButton.IsEnabled = editable && undo.Count > 0;
            RedoButton.IsEnabled = editable && redo.Count > 0;
            BoxName.Text = draft?.Name ?? "";
            // The style is chosen when a preset is made, and only shown.
            var style = StyleOf(free ? "free" : "columns");
            StyleShown.Text = draft is null ? "" : style.Title;
            StyleIcon.Data = draft is null ? null : style.Icon;
            BoxWidth.Text = draft is null ? "" : Number(draft.Width);
            BoxWidth.Visibility = FitButton.Visibility = free ? Visibility.Visible : Visibility.Collapsed;
            BoxWidth.IsEnabled = FitButton.IsEnabled = editable;
            WidthShown.Visibility = free ? Visibility.Collapsed : Visibility.Visible;
            BoxNote.Text = Loc.T(draft is null ? "editor.note.none"
                : sample ? "editor.note.sample"
                : free ? "editor.note.free"
                : "editor.note.columns");
            ShowUse();
            ListPresets();
            ListBlocks();
            FillBlock();
        }
        finally
        {
            loading = false;
        }
        DrawStage();
    }

    /// <summary>Whether the taskbar shows the open preset, or a button that puts it there.</summary>
    void ShowUse()
    {
        var inUse = draft is not null && opened != "" && known?.Look == "box:" + opened;
        InUseBadge.Visibility = inUse ? Visibility.Visible : Visibility.Collapsed;
        UseButton.Visibility = inUse ? Visibility.Collapsed : Visibility.Visible;
        UseButton.IsEnabled = draft is not null && opened != "";
        UseButton.ToolTip = Loc.Plain(draft is not null && opened == "" ? "editor.box.use.unsaved"
            : dirty ? "editor.box.use.saved.tip"
            : "editor.box.use.tip");
    }

    // ---- The presets.

    static BoxStyle StyleOf(string key) => Styles.FirstOrDefault(s => s.Key == key) ?? Styles[0];

    /// <summary>
    /// The browser's rows on their shelves: the user's own and a new one not saved yet, then the samples; the open
    /// one picked, and its shelf unfolded.
    /// </summary>
    void ListPresets()
    {
        var rows = presets
            .Select(p => new PresetRow(p.Name, p.Name, StyleOf(p.Style), p.Sample, known?.Look == "box:" + p.Name, false, p.Shelf))
            .ToList();
        if (draft is not null && opened == "")
        {
            rows.Add(new PresetRow("", draft.Name.Trim() is { Length: > 0 } name ? name : Loc.Plain("editor.presets.unnamed"),
                StyleOf(draft.Free ? "free" : "columns"), false, false, true, "mine"));
        }
        rows = [.. rows.OrderBy(r => r.Group.Order)];
        // A preset just opened is not hidden on a folded shelf; folding it afterwards is left as it is.
        if (draft is not null && opened != unfoldedFor)
        {
            unfoldedFor = opened;
            if (rows.FirstOrDefault(r => r.Key == opened) is { } open && folded.Remove(open.ShelfKey))
            {
                SendFolded();
                presetRows = "";
            }
        }
        var was = loading;
        loading = true;
        var shown = string.Join("\n", rows.Select(r => $"{r.Key}\t{r.Name}\t{r.Detail}"));
        if (shown != presetRows)
        {
            presetRows = shown;
            PresetList.ItemsSource = rows;
            CollectionViewSource.GetDefaultView(rows).GroupDescriptions.Add(new PropertyGroupDescription(nameof(PresetRow.Group)));
        }
        PresetList.SelectedItem = draft is null
            ? null
            : PresetList.Items.OfType<PresetRow>().FirstOrDefault(r => r.Key == opened);
        loading = was;
    }

    /// <summary>The shelves folded, by their words, as the controller keeps them.</summary>
    HashSet<string> folded = ["ai", "pc"];
    /// <summary>While a shelf's head takes its state, its change is not sent.</summary>
    bool folding;
    /// <summary>The preset whose shelf was last unfolded as it opened.</summary>
    string? unfoldedFor;

    void ShelfLoaded(object sender, RoutedEventArgs e)
    {
        if (sender is Expander { DataContext: CollectionViewGroup { Name: Shelf shelf } } head)
        {
            folding = true;
            head.IsExpanded = !folded.Contains(shelf.Key);
            folding = false;
        }
    }

    void ShelfToggled(object sender, RoutedEventArgs e)
    {
        if (folding || e.OriginalSource != sender
            || sender is not Expander { DataContext: CollectionViewGroup { Name: Shelf shelf } } head)
        {
            return;
        }
        if (head.IsExpanded)
        {
            folded.Remove(shelf.Key);
        }
        else
        {
            folded.Add(shelf.Key);
        }
        SendFolded();
    }

    void SendFolded() =>
        send(string.Join("\t", new[] { "folded" }.Concat(Shelves.Where(folded.Contains))));

    static readonly string[] Shelves = ["mine", "basic", "ai", "pc"];

    void PresetPicked(object sender, SelectionChangedEventArgs e)
    {
        if (loading || PresetList.SelectedItem is not PresetRow row || (draft is not null && row.Key == opened))
        {
            return;
        }
        if (!MayDrop())
        {
            ListPresets();
            return;
        }
        send($"edit\t{row.Key}");
    }

    void CreateClicked(object sender, RoutedEventArgs e)
    {
        if (CreateLayer.Visibility == Visibility.Visible || !MayDrop())
        {
            return;
        }
        NewName.Text = Unique(Loc.Plain("editor.presets.first_name"));
        CreateNote.Text = "";
        if (!StyleCards.Children.OfType<RadioButton>().Any(c => c.IsChecked == true))
        {
            ((RadioButton)StyleCards.Children[0]).IsChecked = true;
        }
        FillStarts();
        CreateLayer.Visibility = Visibility.Visible;
        NewName.Focus();
        NewName.SelectAll();
    }

    void CreateConfirmed(object sender, RoutedEventArgs e)
    {
        var name = NewName.Text.Trim();
        var style = PickedStyle();
        var from = (StartBox.SelectedItem as StartRow)?.Name ?? "";
        if (name.Length == 0)
        {
            CreateNote.Text = Loc.T("editor.create.unnamed");
            return;
        }
        if (presets.Any(p => p.Name == name))
        {
            CreateNote.Text = Loc.T("editor.create.taken", ("name", name));
            return;
        }
        pendingName = name;
        CreateLayer.Visibility = Visibility.Collapsed;
        send($"new\t{style}\t{from}");
    }

    void CreateCancelled(object sender, RoutedEventArgs e) => CreateLayer.Visibility = Visibility.Collapsed;

    string PickedStyle() =>
        StyleCards.Children.OfType<RadioButton>().FirstOrDefault(c => c.IsChecked == true)?.Tag as string ?? Styles[0].Key;

    /// <summary>
    /// The presets of the style picked that a new one may start from, the samples first, in the
    /// presets' order; the style's own sample, the first, is picked.
    /// </summary>
    void FillStarts()
    {
        var style = PickedStyle();
        var rows = presets
            .Where(p => p.Style == style)
            .OrderBy(p => p.Sample ? 0 : 1)
            .Select(p => new StartRow(p.Name, Loc.Plain(p.Sample ? "editor.presets.samples" : "editor.presets.mine")))
            .ToList();
        StartBox.ItemsSource = rows;
        CollectionViewSource.GetDefaultView(rows).GroupDescriptions.Add(new PropertyGroupDescription(nameof(StartRow.Group)));
        StartBox.SelectedIndex = rows.Count > 0 ? 0 : -1;
    }

    void BuildStyleCards()
    {
        foreach (var style in Styles)
        {
            var about = new TextBlock { Text = style.About, FontSize = 12, TextWrapping = TextWrapping.Wrap };
            about.SetResourceReference(TextBlock.ForegroundProperty, "TextFillColorSecondaryBrush");
            var card = new RadioButton
            {
                Style = (Style)FindResource("StyleCard"),
                GroupName = "style",
                Tag = style.Key,
                Content = new StackPanel
                {
                    Children =
                    {
                        new Border
                        {
                            Height = 64,
                            CornerRadius = new CornerRadius(6),
                            Background = Frozen(Color.FromRgb(0x0B, 0x0B, 0x0C)),
                            Child = new Shapes.Path
                            {
                                Data = style.Icon,
                                Fill = Accent,
                                Stretch = Stretch.Uniform,
                                Height = 34,
                                HorizontalAlignment = HorizontalAlignment.Center,
                                VerticalAlignment = VerticalAlignment.Center,
                            },
                        },
                        new TextBlock { Text = style.Title, FontWeight = FontWeights.SemiBold, Margin = new Thickness(0, 10, 0, 2) },
                        about,
                    },
                },
            };
            AutomationProperties.SetName(card, style.Title);
            card.MouseDoubleClick += (_, _) => CreateConfirmed(card, new RoutedEventArgs());
            card.Checked += (_, _) => FillStarts();
            StyleCards.Children.Add(card);
        }
    }

    void DuplicateBox(object sender, RoutedEventArgs e)
    {
        if (draft is null || !MayDrop())
        {
            return;
        }
        var copy = draft.Copy();
        copy.Name = Unique(Loc.Plain("editor.presets.copy_name", ("name", draft.Name)));
        Begin(copy);
    }

    /// <summary>
    /// Shows a box just opened or begun with no block chosen and the block
    /// list at its top, as PowerPoint opens a slide.
    /// </summary>
    void StartUnchosen()
    {
        chosen.Clear();
        into = null;
        primary = -1;
        SaveNote.Text = "";
        ShowEditor();
        if (BlockList.Items.Count > 0)
        {
            BlockList.ScrollIntoView(BlockList.Items[0]);
        }
        SendDraft();
    }

    /// <summary>Opens a box not saved yet, a new one or a copy.</summary>
    void Begin(BoxModel box)
    {
        fitting = true;
        centering = true;
        draft = box;
        opened = "";
        sample = false;
        dirty = true;
        stale = true;
        undo.Clear();
        redo.Clear();
        StartUnchosen();
    }

    void DeleteBox(object sender, RoutedEventArgs e)
    {
        if (draft is null || sample || opened == "")
        {
            return;
        }
        var name = opened;
        if (MessageBox.Show(this, Loc.Plain("message.delete_preset", ("name", name)), "LimitPeeker",
                MessageBoxButton.YesNo, MessageBoxImage.Question) == MessageBoxResult.Yes)
        {
            send($"delete\t{name}");
        }
    }

    void SaveBox(object sender, RoutedEventArgs e)
    {
        if (draft is null || sample)
        {
            return;
        }
        // What is being typed in a place is kept inside the box first.
        if (Keyboard.FocusedElement is TextBox typing)
        {
            Committed(typing);
        }
        var json = draft.ToJson();
        SaveNote.Text = Loc.T("editor.save.saving");
        send($"save\t{opened}\t{json}");
    }

    /// <summary>Puts the open preset, as last saved, on the taskbar, as the settings window picks a look.</summary>
    void UseClicked(object sender, RoutedEventArgs e)
    {
        if (draft is null || opened == "")
        {
            return;
        }
        send($"look\tbox:{opened}");
        SaveNote.Text = "";
    }

    /// <summary>Whether what is not saved may go: asked only when there is some.</summary>
    internal bool MayDrop() =>
        !dirty || draft is null || sample
        || MessageBox.Show(this, Loc.Plain("message.drop_changes"), "LimitPeeker",
            MessageBoxButton.YesNo, MessageBoxImage.Question) == MessageBoxResult.Yes;

    string Unique(string name)
    {
        var taken = presets.Select(p => p.Name).ToHashSet();
        name = name.Length > 36 ? name[..36] : name;
        var unique = name;
        for (var n = 2; taken.Contains(unique); n++)
        {
            unique = $"{name} {n}";
        }
        return unique;
    }

    /// <summary>A change: the scene is asked for again once the changes pause.</summary>
    void Touch()
    {
        dirty = true;
        SaveButton.IsEnabled = !sample;
        UndoButton.IsEnabled = !sample && undo.Count > 0;
        RedoButton.IsEnabled = !sample && redo.Count > 0;
        SaveNote.Text = "";
        ShowUse();
        ListPresets();
        ListBlocks();
        DrawStage();
        drafting.Stop();
        drafting.Start();
    }

    void SendDraft()
    {
        if (draft is null)
        {
            return;
        }
        sentPlaces = Enumerable.Range(0, draft.Blocks.Count).ToDictionary(i => i, i => new Point(draft.Blocks[i].X, draft.Blocks[i].Y));
        var json = draft.ToJson();
        send($"draft\t{json}");
    }

    /// <summary>The controller's scene: the box, each item and the block it comes from, or what is wrong.</summary>
    internal void ShowPreview(string text)
    {
        var notes = new List<string>();
        var slots = new Dictionary<int, (string Id, uint Color)>();
        frame = null;
        items.Clear();
        foreach (var line in text.Split('\n'))
        {
            switch (line.Split('\t'))
            {
                case ["error", var why]:
                    notes.Add(why);
                    break;
                case ["stand-in", var name]:
                    notes.Add(Loc.T("editor.preview.stand_in", ("service", name)));
                    break;
                case ["box", var width, var height, var padding, var radius, var color, var track]:
                    frame = new BoxFrame(Int(width), Int(height), Int(padding), Int(radius), UInt(color), UInt(track));
                    break;
                case ["icon", var slot, var id, var color]:
                    slots[Int(slot)] = (id, UInt(color));
                    break;
                case ["item", .. var rest] when rest.Length == 14:
                    items.Add(new SceneItem(Int(rest[0]), Int(rest[1]), Int(rest[2]), Int(rest[3]), Int(rest[4]),
                        Int(rest[5]), Int(rest[6]), UInt(rest[7]), UInt(rest[8]), Int(rest[9]), Int(rest[10]),
                        Int(rest[11]), Int(rest[12]), rest[13]));
                    break;
            }
        }
        icons = slots;
        shownPlaces = sentPlaces;
        stale = false;
        PreviewNote.Text = string.Join(" ", notes);
        HeightShown.Text = Loc.Plain("editor.box.height.fixed", ("height", Number(Tall)));
        WidthShown.Text = frame is { } shown ? Loc.Plain("editor.box.width.columns", ("width", Number(shown.Width / 100.0))) : "";
        if (fitting && frame is { } box)
        {
            fitting = false;
            centering = true;
            // The largest zoom whose box and sizes fit the stage across.
            var outer = (box.Width + 2 * box.Padding) / 100.0;
            var room = StageView.ActualWidth - 2 * Room - 24;
            Zoom = new[] { 4.0, 3.0 }.FirstOrDefault(z => outer * z <= room, 2);
            ShowZoom();
        }
        if (Primary is { } block && draft is { Free: true })
        {
            ShowSize(block);
        }
        DrawStage();
    }
}
