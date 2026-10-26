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

// The box editor's blocks: their list and panel, steps back and forward, adding and removing them, and the palette.
public partial class EditorWindow
{
    // Which column each block of a column box is in, and whether it is the
    // lower of two, as the file will write them.
    List<(int Column, bool Lower)> Columns()
    {
        var places = new List<(int, bool)>();
        var column = -1;
        var count = 0;
        foreach (var block in draft?.Blocks ?? [])
        {
            if (block.Stacked && count == 1)
            {
                places.Add((column, true));
                count = 2;
            }
            else
            {
                column++;
                places.Add((column, false));
                count = 1;
            }
        }
        return places;
    }

    void ListBlocks()
    {
        var was = loading;
        loading = true;
        var rows = new List<BlockRow>();
        var free = draft?.Free == true;
        if (draft is not null)
        {
            var columns = Columns();
            for (var at = 0; at < draft.Blocks.Count; at++)
            {
                var where = free
                    ? draft.Blocks[at].Group is { } group ? Loc.Plain("editor.blocks.group", ("group", group)) : ""
                    : Loc.Plain(columns[at].Lower ? "editor.blocks.column.lower" : "editor.blocks.column",
                        ("column", columns[at].Column + 1));
                rows.Add(new BlockRow(at, Describe(draft.Blocks[at]), where));
            }
        }
        // A free box lists the block in front first, as PowerPoint's selection
        // pane and Figma's layers do; its file keeps the one drawn last last.
        if (free)
        {
            rows.Reverse();
        }
        // The rows are made again only when they change, and the pick is
        // changed row by row, so a click in the list is not answered by a
        // list made anew under it.
        if (BlockList.ItemsSource is List<BlockRow> listed && listed.SequenceEqual(rows))
        {
            rows = listed;
        }
        else
        {
            BlockList.ItemsSource = rows;
        }
        foreach (var row in BlockList.SelectedItems.OfType<BlockRow>().Where(r => !chosen.Contains(r.Index)).ToList())
        {
            BlockList.SelectedItems.Remove(row);
        }
        foreach (var row in rows.Where(r => chosen.Contains(r.Index) && !BlockList.SelectedItems.Contains(r)))
        {
            BlockList.SelectedItems.Add(row);
        }
        // The block the panel shows is brought into the list's view.
        if (rows.FirstOrDefault(r => r.Index == primary) is { } shown)
        {
            BlockList.ScrollIntoView(shown);
        }
        var editable = draft is not null && !sample;
        RemoveButton.IsEnabled = editable && chosen.Count > 0;
        // The order the list keeps: in columns, which column is further left;
        // in a free box, which is drawn over which, the front first. Any pick
        // moves, a group too.
        UpButton.IsEnabled = editable && CanReorder(free);
        DownButton.IsEnabled = editable && CanReorder(!free);
        UpButton.ToolTip = Loc.Plain(free ? "editor.blocks.up.free.tip" : "editor.blocks.up.columns.tip");
        DownButton.ToolTip = Loc.Plain(free ? "editor.blocks.down.free.tip" : "editor.blocks.down.columns.tip");
        AddButton.ToolTip = Loc.Plain(free ? "editor.blocks.add.free.tip" : "editor.blocks.add.columns.tip");
        BlockOrder.Text = draft is null ? "" : Loc.T(free ? "editor.blocks.order.free" : "editor.blocks.order.columns");
        GroupButton.IsEnabled = editable && Units(chosen).Count > 1;
        UngroupButton.IsEnabled = editable && chosen.Any(i => draft!.Blocks[i].Group is not null);
        PickedNote.Text = into is { } entered && chosen.Count == 1
            ? Loc.T("editor.blocks.picked.entered", ("group", entered))
            : Units(chosen) is [var whole] && whole.Count > 1 && draft!.Blocks[whole[0]].Group is { } number
                ? Loc.T("editor.blocks.picked.group", ("group", number), ("count", whole.Count))
                : chosen.Count > 1 ? Loc.T("editor.blocks.picked.many", ("count", chosen.Count)) : "";
        loading = was;
    }

    string Describe(BlockModel block)
    {
        var kind = Kinds.FirstOrDefault(k => k.Key == block.Kind)?.Title ?? block.Kind;
        var service = ServiceName(block.Service);
        var detail = block.Kind switch
        {
            "space" => "",
            "icon" => service,
            "text" when block.Own is { } own => $"\"{own}\"",
            "text" when block.Show == "name" => service,
            _ => Loc.Plain("editor.blocks.service_limits", ("service", service),
                ("limits", string.Join($" {Loc.Plain("editor.blocks.and")} ",
                    block.Limits.Take(block.Kind == "rings" ? 3 : 1).Select(LimitTitle)))),
        };
        return $"{kind}{(detail == "" ? "" : " · " + detail)}";
    }

    string ServiceName(string? id) =>
        known?.Services.FirstOrDefault(s => s.Id == id)?.Name ?? id ?? Loc.Plain("editor.blocks.no_service");

    static string LimitTitle(Which which) => which.Label ?? which.Nth switch
    {
        1 => Loc.Plain("editor.limit.first"),
        2 => Loc.Plain("editor.limit.second"),
        3 => Loc.Plain("editor.limit.third"),
        var n => Loc.Plain("editor.limit.nth", ("n", n)),
    };

    /// <summary>The limits a list offers: by place, then the service's own by name, and the one chosen.</summary>
    List<Option> LimitOptions(string? service, bool none, Which? chosenLimit)
    {
        var options = new List<Option>();
        if (none)
        {
            options.Add(new Option("", Loc.Plain("editor.limit.none")));
        }
        options.AddRange(new[] { 1, 2, 3 }.Select(n => new Option($"#{n}", LimitTitle(new Which(n, null)))));
        foreach (var limit in known?.Services.FirstOrDefault(s => s.Id == service)?.Limits ?? [])
        {
            options.Add(new Option("=" + limit.Label, limit.Label));
        }
        if (chosenLimit is not null && options.All(o => o.Key != chosenLimit.Key))
        {
            options.Add(new Option(chosenLimit.Key, LimitTitle(chosenLimit)));
        }
        return options;
    }

    List<Option> ServiceOptions(string? service)
    {
        var options = (known?.Services ?? []).Select(s => new Option(s.Id, s.Name)).ToList();
        if (service is not null && options.All(o => o.Key != service))
        {
            options.Add(new Option(service, service));
        }
        return options;
    }

    BlockModel? Primary => draft is not null && primary >= 0 && primary < draft.Blocks.Count ? draft.Blocks[primary] : null;

    /// <summary>Fills the block's controls from the block the panel shows, and shows those its kind takes.</summary>
    void FillBlock()
    {
        var was = loading;
        loading = true;
        try
        {
            var block = Primary;
            BlockPanel.Visibility = block is null ? Visibility.Collapsed : Visibility.Visible;
            BlockTitle.Visibility = BlockPanel.Visibility;
            // Where a block's settings will be, while none is chosen.
            NoBlockNote.Visibility = block is null && draft is { Blocks.Count: > 0 } ? Visibility.Visible : Visibility.Collapsed;
            if (block is null)
            {
                return;
            }
            KindBox.SelectedValue = block.Kind;
            ServiceBox.ItemsSource = ServiceOptions(block.Service);
            ServiceBox.SelectedValue = block.Service;
            var first = block.Limits.ElementAtOrDefault(0) ?? Which.First;
            LimitBox.ItemsSource = LimitOptions(block.Service, false, first);
            LimitBox.SelectedValue = first.Key;
            var second = block.Limits.ElementAtOrDefault(1);
            Limit2Box.ItemsSource = LimitOptions(block.Service, true, second);
            Limit2Box.SelectedValue = second?.Key ?? "";
            var third = block.Limits.ElementAtOrDefault(2);
            Limit3Box.ItemsSource = LimitOptions(block.Service, true, third);
            Limit3Box.SelectedValue = third?.Key ?? "";
            ShowBox.SelectedValue = block.Own is not null ? "own" : block.Show;
            OwnText.Text = block.Own ?? "";
            SizeText.Text = Number(block.Size);
            var weight = ((int)(block.Weight ?? 600)).ToString(CultureInfo.InvariantCulture);
            WeightBox.ItemsSource = Weights.Any(w => w.Key == weight) ? Weights : Weights.Append(new Option(weight, weight)).ToList();
            WeightBox.SelectedValue = weight;
            AlignBox.SelectedValue = block.Align;
            WidthText.Text = Number(block.Width);
            ThicknessText.Text = Number(block.Thickness);
            ColorText.Text = block.Color ?? "";
            StackedCheck.IsChecked = block.Stacked;
            XText.Text = Number(block.X);
            YText.Text = Number(block.Y);
            ShowSize(block, always: true);
            ShowRows(block);
        }
        finally
        {
            loading = was;
        }
    }

    /// <summary>
    /// The W and H boxes: a block's own width and height when it gives them,
    /// or else the size the controller drew it at, which left as it is stays
    /// the block's own. A box being typed in is left alone.
    /// </summary>
    void ShowSize(BlockModel block, bool always = false)
    {
        var drawn = stale || primary < 0 ? Rect.Empty : Drawn(primary);
        ownW = drawn.IsEmpty ? "" : Number(Math.Round(drawn.Width, 1));
        ownH = drawn.IsEmpty ? "" : Number(Math.Round(drawn.Height, 1));
        var was = loading;
        loading = true;
        if (always || !WText.IsKeyboardFocusWithin)
        {
            WText.Text = block.W is { } w ? Number(w) : ownW;
        }
        if (always || !HText.IsKeyboardFocusWithin)
        {
            HText.Text = block.H is { } h ? Number(h) : ownH;
        }
        loading = was;
    }

    void ShowRows(BlockModel block)
    {
        static Visibility When(bool shown) => shown ? Visibility.Visible : Visibility.Collapsed;
        var free = draft?.Free == true;
        var text = block.Kind == "text";
        ServiceRow.Visibility = When(block.NamesService);
        LimitRow.Visibility = When(block.ReadsLimit);
        MoreLimitsRow.Visibility = When(block.Kind == "rings");
        ShowRow.Visibility = When(text);
        OwnRow.Visibility = When(text && block.Own is not null);
        FontRow.Visibility = When(text);
        WidthRow.Visibility = When(!free && block.Kind is "bar" or "space");
        Hint.On(WidthText, block.Kind == "space" ? "6" : "40");
        ThicknessRow.Visibility = When(block.Kind is "bar" or "ring" or "rings");
        ColorRow.Visibility = When(block.Kind is not ("space" or "icon"));
        LookHeading.Visibility = When(new[] { WidthRow, ThicknessRow, ColorRow }.Any(r => r.Visibility == Visibility.Visible));
        StackRow.Visibility = When(!free && primary > 0);
        PlaceRow.Visibility = When(free);
        ColorSwatch.Background = ParseColor(block.Color) is { } color ? new SolidColorBrush(color) : Brushes.Transparent;
    }

    /// <summary>Keeps what the box was before a change, so Undo can bring it back; typing keeps one for each pause.</summary>
    void Checkpoint(bool typing = false)
    {
        if (draft is null || (typing && (DateTime.Now - lastEdit).TotalSeconds < 1))
        {
            lastEdit = DateTime.Now;
            return;
        }
        lastEdit = DateTime.Now;
        var now = draft.ToJson();
        if (undo.Count == 0 || undo.Peek().Json != now)
        {
            undo.Push((now, primary));
        }
        redo.Clear();
    }

    void UndoClicked(object sender, RoutedEventArgs e) => Step(undo, redo);
    void RedoClicked(object sender, RoutedEventArgs e) => Step(redo, undo);

    void Step(Stack<(string Json, int Primary)> from, Stack<(string Json, int Primary)> to)
    {
        if (draft is null || sample || from.Count == 0)
        {
            return;
        }
        to.Push((draft.ToJson(), primary));
        var (json, was) = from.Pop();
        draft = BoxModel.Parse(json);
        // What is picked again is the block the panel showed at that step,
        // with its group.
        into = null;
        primary = Math.Min(was, draft.Blocks.Count - 1);
        chosen.Clear();
        foreach (var unit in Units(primary >= 0 ? [primary] : []))
        {
            chosen.UnionWith(unit);
        }
        dirty = true;
        stale = true;
        ShowEditor();
        SendDraft();
    }

    /// <summary>Takes a change in the block's controls into the block.</summary>
    void BlockEdited(object sender, RoutedEventArgs e)
    {
        if (loading || Primary is not { } block)
        {
            return;
        }
        Checkpoint(typing: sender is TextBox);
        var kind = KindBox.SelectedValue as string ?? block.Kind;
        var kindChanged = kind != block.Kind;
        block.Kind = kind;
        var service = ServiceBox.SelectedValue as string;
        var serviceChanged = service != block.Service;
        block.Service = service ?? block.Service ?? known?.Services.FirstOrDefault()?.Id;
        var limits = new List<Which> { Which.FromKey(LimitBox.SelectedValue as string) ?? Which.First };
        if (block.Kind == "rings")
        {
            limits.AddRange(new[] { Limit2Box, Limit3Box }.Select(b => Which.FromKey(b.SelectedValue as string)).OfType<Which>());
        }
        block.Limits = limits;
        var shows = ShowBox.SelectedValue as string ?? "value";
        if (shows == "own")
        {
            block.Own = OwnText.Text.Length > 0 ? OwnText.Text : block.Own ?? "Text";
        }
        else
        {
            block.Own = null;
            block.Show = shows;
        }
        block.Size = Parse(SizeText.Text);
        block.Weight = WeightBox.SelectedValue is string weight ? double.Parse(weight, CultureInfo.InvariantCulture) : null;
        block.Align = AlignBox.SelectedValue as string ?? "start";
        block.Width = Parse(WidthText.Text);
        block.Thickness = Parse(ThicknessText.Text);
        block.Color = ColorText.Text.Trim() is { Length: > 0 } color ? color : null;
        block.Stacked = StackedCheck.IsChecked == true;
        block.X = Parse(XText.Text) ?? 0;
        block.Y = Parse(YText.Text) ?? 0;
        // The size shown for a block without its own stays its own until changed.
        block.W = block.W is null && WText.Text == ownW ? null : Parse(WText.Text);
        block.H = block.H is null && HText.Text == ownH ? null : Parse(HText.Text);
        if (kindChanged && draft!.Free)
        {
            // Another kind takes its own size, where it was.
            block.W = block.H = null;
            stale = true;
        }
        if (serviceChanged || sender == KindBox || sender == ShowBox)
        {
            FillBlock();
        }
        else
        {
            ShowRows(block);
        }
        Touch();
    }

    /// <summary>Takes a change in the box's name or width.</summary>
    void BoxEdited(object sender, RoutedEventArgs e)
    {
        if (loading || draft is null || sample)
        {
            return;
        }
        Checkpoint(typing: true);
        draft.Name = BoxName.Text;
        draft.Width = Parse(BoxWidth.Text) ?? draft.Width;
        Touch();
    }

    void BlockPicked(object sender, SelectionChangedEventArgs e)
    {
        if (loading)
        {
            return;
        }
        var rows = BlockList.SelectedItems.OfType<BlockRow>().Select(r => r.Index).ToList();
        var last = e.AddedItems.OfType<BlockRow>().LastOrDefault()?.Index ?? rows.LastOrDefault(-1);
        // A block of the group picked whole, clicked alone in the list, is
        // picked into the group, as on the stage.
        if (rows is [var one] && Units(chosen) is [var whole] && whole.Count > 1 && whole.Contains(one))
        {
            Enter(one);
            return;
        }
        Choose(rows, last);
    }

    /// <summary>Picks one block of a group alone, the group entered, as a shape of a group clicked again on a slide.</summary>
    void Enter(int block)
    {
        into = draft!.Blocks[block].Group;
        enterAt = null;
        chosen.Clear();
        chosen.Add(block);
        primary = block;
        ListBlocks();
        FillBlock();
        DrawStage();
    }

    /// <summary>Picks these blocks, whole groups at a time, the panel showing `shown`.</summary>
    void Choose(IEnumerable<int> blocks, int shown)
    {
        var list = blocks.ToList();
        // A group entered is left once nothing, or anything outside it, is
        // picked.
        if (into is { } entered && (draft is null || list.Count == 0
            || list.Any(i => i < 0 || i >= draft.Blocks.Count || draft.Blocks[i].Group != entered)))
        {
            into = null;
        }
        var units = Units(list);
        chosen.Clear();
        foreach (var unit in units)
        {
            chosen.UnionWith(unit);
        }
        primary = chosen.Contains(shown) ? shown : chosen.DefaultIfEmpty(-1).Max();
        ListBlocks();
        FillBlock();
        DrawStage();
    }

    /// <summary>The picked blocks as they move: a block alone, or its whole group, unless the group is entered.</summary>
    List<List<int>> Units(IEnumerable<int> blocks)
    {
        var units = new List<List<int>>();
        if (draft is null)
        {
            return units;
        }
        var seen = new HashSet<int>();
        foreach (var at in blocks.Where(b => b >= 0 && b < draft.Blocks.Count).OrderBy(b => b))
        {
            if (seen.Contains(at))
            {
                continue;
            }
            var group = draft.Free && draft.Blocks[at].Group is { } g && g != into ? g : (int?)null;
            var unit = group is null
                ? [at]
                : Enumerable.Range(0, draft.Blocks.Count).Where(i => draft.Blocks[i].Group == group).ToList();
            seen.UnionWith(unit);
            units.Add(unit);
        }
        return units;
    }

    /// <summary>The kinds a block may be added as: a free box has no spaces.</summary>
    void AddClicked(object sender, RoutedEventArgs e)
    {
        if (draft is null || sample)
        {
            return;
        }
        var menu = new ContextMenu { PlacementTarget = AddButton, Placement = PlacementMode.Bottom };
        foreach (var kind in Kinds.Where(k => !(draft.Free && k.Key == "space")))
        {
            var item = new MenuItem { Header = kind.Title };
            item.Click += (_, _) => AddBlock(kind.Key);
            menu.Items.Add(item);
        }
        menu.IsOpen = true;
    }

    /// <summary>About how large a block of a kind is drawn at its own size, in DIPs, until the controller says.</summary>
    static (double W, double H) Guess(string kind) => kind switch
    {
        "icon" => (16, 16),
        "pie" => (20, 20),
        "bar" => (40, 4),
        "text" => (40, 14),
        _ => (32, 32),
    };

    void AddBlock(string kind)
    {
        if (draft is null || sample)
        {
            return;
        }
        Checkpoint();
        var block = new BlockModel
        {
            Kind = kind,
            Service = Primary?.Service ?? known?.Services.FirstOrDefault()?.Id ?? "claude",
        };
        if (block.Kind == "text")
        {
            block.Show = "both";
        }
        if (draft.Free)
        {
            // In the middle of the box, inside it, to be dragged where it goes.
            var (w, h) = Guess(kind);
            block.X = Math.Max(0, Math.Round(draft.Width / 2 - w / 2));
            block.Y = Math.Max(0, Math.Round((Tall - h) / 2, 1));
            draft.Width = Math.Min(Widest, Math.Max(draft.Width, block.X + w));
        }
        var at = primary < 0 ? draft.Blocks.Count : primary + 1;
        draft.Blocks.Insert(at, block);
        stale = true;
        Choose([at], at);
        ShowWidth();
        Touch();
    }

    void RemoveBlocks(object sender, RoutedEventArgs e)
    {
        if (draft is null || sample || chosen.Count == 0)
        {
            return;
        }
        Checkpoint();
        foreach (var at in chosen.OrderByDescending(i => i))
        {
            draft.Blocks.RemoveAt(at);
        }
        stale = true;
        var next = Math.Min(chosen.Min(), draft.Blocks.Count - 1);
        Choose(next >= 0 ? [next] : [], next);
        Touch();
    }

    // Up the list: in columns earlier in the file, toward the left; in a free
    // box later, in front, as its list shows the front first.
    void MoveBlockUp(object sender, RoutedEventArgs e) => Reorder(draft?.Free == true);
    void MoveBlockDown(object sender, RoutedEventArgs e) => Reorder(draft?.Free != true);

    /// <summary>Whether some picked block has one not picked next to it, later in the file or earlier.</summary>
    bool CanReorder(bool later) =>
        draft is not null && chosen.Any(i => later
            ? i + 1 < draft.Blocks.Count && !chosen.Contains(i + 1)
            : i > 0 && !chosen.Contains(i - 1));

    /// <summary>
    /// Moves each picked block past the next block not picked, later in the
    /// file or earlier, the picked keeping their order among themselves, as
    /// PowerPoint brings a selection forward or sends it back. A group moves
    /// as its blocks do, wherever they are.
    /// </summary>
    void Reorder(bool later)
    {
        if (draft is null || sample || !CanReorder(later))
        {
            return;
        }
        Checkpoint();
        var blocks = draft.Blocks;
        var picked = chosen.Select(i => blocks[i]).ToHashSet();
        var shown = Primary;
        if (later)
        {
            for (var i = blocks.Count - 2; i >= 0; i--)
            {
                if (picked.Contains(blocks[i]) && !picked.Contains(blocks[i + 1]))
                {
                    (blocks[i], blocks[i + 1]) = (blocks[i + 1], blocks[i]);
                }
            }
        }
        else
        {
            for (var i = 1; i < blocks.Count; i++)
            {
                if (picked.Contains(blocks[i]) && !picked.Contains(blocks[i - 1]))
                {
                    (blocks[i], blocks[i - 1]) = (blocks[i - 1], blocks[i]);
                }
            }
        }
        chosen.Clear();
        chosen.UnionWith(picked.Select(b => blocks.IndexOf(b)));
        primary = shown is null ? -1 : blocks.IndexOf(shown);
        stale = true;
        FillBlock();
        Touch();
    }


    // ---- The palette.

    void BuildPalette()
    {
        foreach (var (panel, colors) in new[] { (Palette, Hues), (GreyPalette, Greys) })
        {
            foreach (var hex in colors)
            {
                var swatch = new Button
                {
                    Style = (Style)FindResource("Swatch"),
                    Background = Frozen(ParseColor(hex)!.Value),
                    Tag = hex,
                    ToolTip = hex,
                };
                AutomationProperties.SetName(swatch, hex);
                swatch.Click += SwatchClicked;
                panel.Children.Add(swatch);
            }
        }
    }

    /// <summary>Opens the palette, the block's color checked in it.</summary>
    void ColorClicked(object sender, RoutedEventArgs e)
    {
        var current = ColorText.Text.Trim();
        foreach (var swatch in Palette.Children.OfType<Button>().Concat(GreyPalette.Children.OfType<Button>()))
        {
            var hex = (string)swatch.Tag;
            var dark = ParseColor(hex) is { } c && c.R * 0.3 + c.G * 0.59 + c.B * 0.11 > 150;
            swatch.Content = string.Equals(hex, current, StringComparison.OrdinalIgnoreCase)
                ? new TextBlock { Text = "", FontFamily = (FontFamily)FindResource("Icons"), FontSize = 12, Foreground = dark ? Brushes.Black : Brushes.White }
                : null;
        }
        ColorPopup.IsOpen = true;
    }

    void SwatchClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string hex })
        {
            ColorPopup.IsOpen = false;
            ColorText.Text = hex;
        }
    }

    void ServiceColorClicked(object sender, RoutedEventArgs e)
    {
        ColorPopup.IsOpen = false;
        ColorText.Text = "";
    }

    static int Int(string text) => int.TryParse(text, CultureInfo.InvariantCulture, out var n) ? n : 0;
    static uint UInt(string text) => uint.TryParse(text, CultureInfo.InvariantCulture, out var n) ? n : 0;
    static double? Parse(string text) =>
        double.TryParse(text.Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var n) ? n : null;
    static string Number(double? value) => value is { } v ? v.ToString("0.##", CultureInfo.InvariantCulture) : "";

    static SolidColorBrush Brush(uint argb) =>
        new(Color.FromArgb((byte)(argb >> 24), (byte)(argb >> 16), (byte)(argb >> 8), (byte)argb));

    static Brush Frozen(Color color)
    {
        var brush = new SolidColorBrush(color);
        brush.Freeze();
        return brush;
    }

    static Geometry Picture(string data)
    {
        var geometry = Geometry.Parse(data);
        geometry.Freeze();
        return geometry;
    }

    static Color? ParseColor(string? text) =>
        text is ['#', .. var hex] && hex.Length == 6 && uint.TryParse(hex, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var rgb)
            ? Color.FromRgb((byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb)
            : null;
}
