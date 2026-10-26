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

// The box editor's stage: where things are, keeping blocks inside the box, and drawing it.
public partial class EditorWindow
{
    // ---- Where things are. Rects are in DIPs from the box's origin, the
    // top-left corner inside its padding.

    /// <summary>Where the controller drew a block's items: their union.</summary>
    Rect Drawn(int block)
    {
        var rect = Rect.Empty;
        foreach (var item in items.Where(i => i.Block == block))
        {
            rect.Union(new Rect(item.X / 100.0, item.Y / 100.0, item.W / 100.0, item.H / 100.0));
        }
        return rect;
    }

    /// <summary>
    /// A block's place: in a free box where it says, as large as it says or
    /// as the controller drew it; in columns where the controller put it.
    /// </summary>
    Rect Place(int block)
    {
        var drawn = Drawn(block);
        if (draft is not { Free: true } || block >= draft.Blocks.Count)
        {
            return drawn;
        }
        var model = draft.Blocks[block];
        var w = model.W ?? (drawn.IsEmpty || stale ? Guess(model.Kind).W : drawn.Width);
        var h = model.H ?? (drawn.IsEmpty || stale ? Guess(model.Kind).H : drawn.Height);
        return new Rect(model.X, model.Y, w, h);
    }

    Rect UnitPlace(IEnumerable<int> unit)
    {
        var rect = Rect.Empty;
        foreach (var at in unit)
        {
            if (Place(at) is { IsEmpty: false } place)
            {
                rect.Union(place);
            }
        }
        return rect;
    }

    /// <summary>How far right a free box's blocks reach: the least width that holds them.</summary>
    double Reach() =>
        draft is { Free: true }
            ? Enumerable.Range(0, draft.Blocks.Count).Select(Place).Where(p => !p.IsEmpty).Select(p => p.Right).DefaultIfEmpty(0).Max()
            : 0;

    /// <summary>How far a free block has moved since the scene drawn was laid out.</summary>
    Vector Moved(int block) =>
        draft is { Free: true } && block < draft.Blocks.Count && shownPlaces.TryGetValue(block, out var shown)
            ? new Vector(draft.Blocks[block].X - shown.X, draft.Blocks[block].Y - shown.Y)
            : new Vector();

    double Inset => (frame?.Padding ?? 0) / 100.0;

    /// <summary>A point in DIPs from the box's origin, on the stage.</summary>
    Point OnStage(double x, double y) => new(left0 + (Inset + x) * Zoom, top0 + y * Zoom);

    Rect StageRect(Rect rect) => new(OnStage(rect.X, rect.Y), OnStage(rect.Right, rect.Bottom));

    // ---- Keeping a free box's blocks inside it.

    /// <summary>A value between two, the least when there is no room between them.</summary>
    static double Within(double value, double least, double most) =>
        most < least ? least : Math.Min(most, Math.Max(least, value));

    /// <summary>The box's width in its box, unless it is being typed.</summary>
    void ShowWidth()
    {
        if (draft is null || BoxWidth.IsKeyboardFocusWithin)
        {
            return;
        }
        var was = loading;
        loading = true;
        BoxWidth.Text = Number(draft.Width);
        loading = was;
    }

    /// <summary>
    /// A value typed in a place or the box's width, once Enter is pressed or
    /// the box left: a block kept inside the box, which grows to its right,
    /// and the box no narrower than its blocks reach.
    /// </summary>
    void Committed(TextBox box)
    {
        if (loading || draft is not { Free: true } || sample)
        {
            return;
        }
        if (box == BoxWidth)
        {
            var width = Within(draft.Width, Math.Max(1, Math.Ceiling(Reach() * 10) / 10), Widest);
            if (width != draft.Width)
            {
                SaveNote.Text = width > draft.Width ? Loc.T("editor.box.width.kept", ("width", Number(width))) : "";
                draft.Width = width;
                ShowWidth();
                Touch();
            }
            return;
        }
        if (box != XText && box != YText && box != WText && box != HText || Primary is not { } block)
        {
            return;
        }
        var was = (block.X, block.Y, block.W, block.H, draft.Width);
        if (block.W is { } w)
        {
            block.W = Within(w, 1, Widest);
        }
        if (block.H is { } h)
        {
            block.H = Within(h, 1, Tall);
        }
        var place = Place(primary);
        block.X = Within(block.X, 0, Widest - place.Width);
        block.Y = Within(block.Y, 0, Tall - place.Height);
        draft.Width = Math.Min(Widest, Math.Max(draft.Width, block.X + place.Width));
        if (was != (block.X, block.Y, block.W, block.H, draft.Width))
        {
            FillBlock();
            ShowWidth();
            Touch();
        }
    }

    void FieldLeft(object sender, KeyboardFocusChangedEventArgs e) => Committed((TextBox)sender);

    void FitClicked(object sender, RoutedEventArgs e)
    {
        if (draft is not { Free: true } || sample)
        {
            return;
        }
        Checkpoint();
        draft.Width = Within(Math.Ceiling(Reach() * 10) / 10, 1, Widest);
        ShowWidth();
        Touch();
    }

    void OwnSizeClicked(object sender, RoutedEventArgs e)
    {
        if (Primary is not { } block || sample)
        {
            return;
        }
        Checkpoint();
        block.W = block.H = null;
        stale = true;
        FillBlock();
        Touch();
    }

    /// <summary>A rect with a handle moved by (dx, dy), each side inside the box, which may grow to the right.</summary>
    Rect Stretched(Rect start, int grabbed, double dx, double dy)
    {
        var (left, top, right, bottom) = (start.Left, start.Top, start.Right, start.Bottom);
        // Clockwise from the top-left: which sides each handle moves.
        if (grabbed is 0 or 6 or 7) left = Within(Math.Round(left + dx), 0, right - 1);
        if (grabbed is 2 or 3 or 4) right = Within(Math.Round(right + dx), left + 1, Widest);
        if (grabbed is 0 or 1 or 2) top = Within(Math.Round(top + dy), 0, bottom - 1);
        if (grabbed is 4 or 5 or 6) bottom = Within(Math.Round(bottom + dy), top + 1, Tall);
        return new Rect(new Point(left, top), new Point(right, bottom));
    }

    /// <summary>
    /// A rect with a handle moved by (dx, dy), its shape kept: a corner holds
    /// the one across from it, a side the middle across it; as large as the
    /// box has room for.
    /// </summary>
    Rect Scaled(Rect start, int grabbed, double dx, double dy)
    {
        var (w, h) = (start.Width, start.Height);
        if (w <= 0 || h <= 0)
        {
            return Stretched(start, grabbed, dx, dy);
        }
        var west = grabbed is 0 or 6 or 7;
        var east = grabbed is 2 or 3 or 4;
        var north = grabbed is 0 or 1 or 2;
        var south = grabbed is 4 or 5 or 6;
        var sx = (w + (east ? dx : west ? -dx : 0)) / w;
        var sy = (h + (south ? dy : north ? -dy : 0)) / h;
        var scale = (west || east) && (north || south)
            ? Math.Abs(sx - 1) >= Math.Abs(sy - 1) ? sx : sy
            : west || east ? sx : sy;
        var (cx, cy) = (start.Left + w / 2, start.Top + h / 2);
        var most = double.MaxValue;
        if (west) most = Math.Min(most, start.Right / w);
        if (east) most = Math.Min(most, (Widest - start.Left) / w);
        if (north) most = Math.Min(most, start.Bottom / h);
        if (south) most = Math.Min(most, (Tall - start.Top) / h);
        if (!north && !south) most = Math.Min(most, 2 * Math.Min(cy, Tall - cy) / h);
        if (!west && !east) most = Math.Min(most, 2 * Math.Min(cx, Widest - cx) / w);
        scale = Within(scale, Math.Max(1 / w, 1 / h), most);
        var (nw, nh) = (w * scale, h * scale);
        var x = west ? start.Right - nw : east ? start.Left : cx - nw / 2;
        var y = north ? start.Bottom - nh : south ? start.Top : cy - nh / 2;
        return new Rect(Math.Max(0, Math.Round(x, 1)), Math.Max(0, Math.Round(y, 1)), Math.Round(nw, 1), Math.Round(nh, 1));
    }

    /// <summary>Moves blocks by (dx, dy) as far as the box lets them, from where they started; the box grows to the right.</summary>
    void Shift(Dictionary<int, Point> from, Rect union, double width, double dx, double dy)
    {
        var mx = Within(dx, -union.Left, Widest - union.Right);
        var my = Within(dy, -union.Top, Tall - union.Bottom);
        foreach (var (block, start) in from)
        {
            draft!.Blocks[block].X = Math.Max(0, Math.Round(start.X + mx, 2));
            draft.Blocks[block].Y = Math.Max(0, Math.Round(start.Y + my, 2));
        }
        draft!.Width = Math.Min(Widest, Math.Max(width, Math.Round(union.Right + mx, 2)));
        ShowWidth();
    }

    // ---- Drawing.

    /// <summary>
    /// Draws the box as the controller laid it out, then over it the origin,
    /// the columns' guides, the picked blocks with their points and handles,
    /// and, when asked, the sizes.
    /// </summary>
    void DrawStage()
    {
        Stage.Children.Clear();
        if (frame is not { } box || draft is null)
        {
            Stage.Width = Stage.Height = 0;
            return;
        }
        var zoom = Zoom;
        // A free box is as wide as the draft says, even while a block pushes it.
        var inside = draft.Free ? draft.Width : box.Width / 100.0;
        var height = box.Height / 100.0;
        var outer = inside + 2 * Inset;
        var reach = Reach(box);
        if (centering && gesture == Gesture.None && StageView.ViewportWidth > 0)
        {
            centering = false;
            left0 = Math.Max(Room, Math.Floor((StageView.ViewportWidth - outer * zoom) / 2));
            top0 = Math.Max(Room, Math.Floor((StageView.ViewportHeight - height * zoom) / 2));
            spanX = spanY = 0;
            // From the start of the stage too, not where a zoom with the wheel scrolled it.
            StageView.ScrollToHorizontalOffset(0);
            StageView.ScrollToVerticalOffset(0);
        }
        Stage.Width = Math.Max(left0 + reach.Width * zoom + Room, spanX);
        Stage.Height = Math.Max(top0 + reach.Height * zoom + Room, spanY);
        Add(new Shapes.Rectangle
        {
            Width = outer * zoom,
            Height = height * zoom,
            RadiusX = box.Radius / 100.0 * zoom,
            RadiusY = box.Radius / 100.0 * zoom,
            Fill = Brush(box.Color),
        }, left0, top0);
        foreach (var item in items)
        {
            var moved = Moved(item.Block);
            var at = OnStage(item.X / 100.0 + moved.X, item.Y / 100.0 + moved.Y);
            Add(Draw(item, box, zoom), at.X, at.Y);
        }
        if (!draft.Free)
        {
            DrawColumns(height);
        }
        // The origin: the top-left corner inside the padding, where X and Y
        // are counted from. White, where the picked blocks' points are blue.
        var origin = OnStage(0, 0);
        Dot(origin, Mark, Accent, 9);
        Callout("0, 0", new Point(origin.X - 7, origin.Y - 22), Mark, before: true);
        // The group entered, faint around the block picked in it.
        if (into is { } entered && draft.Free
            && UnitPlace(Enumerable.Range(0, draft.Blocks.Count).Where(i => draft.Blocks[i].Group == entered)) is { IsEmpty: false } around)
        {
            var edge = StageRect(around);
            Add(new Shapes.Rectangle
            {
                Width = Math.Max(1, edge.Width),
                Height = Math.Max(1, edge.Height),
                Stroke = Guide,
                StrokeThickness = 1,
                StrokeDashArray = [2, 3],
                IsHitTestVisible = false,
            }, edge.X, edge.Y);
        }
        var units = Units(chosen);
        foreach (var unit in units)
        {
            DrawPicked(unit, handles: units.Count == 1);
        }
        if (SizesCheck.IsChecked == true)
        {
            DrawSizes(inside, height);
        }
        if (gesture == Gesture.Marquee)
        {
            Add(new Shapes.Rectangle
            {
                Width = marquee.Width,
                Height = marquee.Height,
                Stroke = Accent,
                StrokeThickness = 1,
                Fill = Frozen(Color.FromArgb(0x26, 0x4C, 0xC2, 0xFF)),
                IsHitTestVisible = false,
            }, marquee.X, marquee.Y);
        }
    }

    /// <summary>A dashed line down the middle of the room between each two columns, and each column's number under the box.</summary>
    void DrawColumns(double height)
    {
        var spans = Columns()
            .Select((place, at) => (place.Column, Rect: Drawn(at)))
            .Where(c => !c.Rect.IsEmpty)
            .GroupBy(c => c.Column)
            .Select(g => (Column: g.Key, Left: g.Min(c => c.Rect.Left), Right: g.Max(c => c.Rect.Right)))
            .OrderBy(c => c.Left)
            .ToList();
        for (var at = 0; at < spans.Count; at++)
        {
            var span = spans[at];
            var under = OnStage((span.Left + span.Right) / 2, height);
            Callout((span.Column + 1).ToString(CultureInfo.InvariantCulture), new Point(under.X - 5, under.Y + 6), Guide, center: true);
            if (at == 0)
            {
                continue;
            }
            var x = (spans[at - 1].Right + span.Left) / 2;
            var top = OnStage(x, 0);
            Add(new Shapes.Line
            {
                X1 = top.X,
                Y1 = top.Y - 6,
                X2 = top.X,
                Y2 = OnStage(x, height).Y + 6,
                Stroke = Guide,
                StrokeThickness = 1,
                StrokeDashArray = [4, 3],
                IsHitTestVisible = false,
            }, 0, 0);
        }
    }

    /// <summary>
    /// A picked block or group: a dashed outline, and in a free box, when it
    /// is all that is picked, the handles that size it. The top-left corner,
    /// which X and Y give, is its point, blue and larger than a handle, and is
    /// the top-left handle too.
    /// </summary>
    void DrawPicked(List<int> unit, bool handles)
    {
        var place = UnitPlace(unit);
        if (place.IsEmpty)
        {
            return;
        }
        var rect = StageRect(place);
        Add(new Shapes.Rectangle
        {
            Width = Math.Max(1, rect.Width),
            Height = Math.Max(1, rect.Height),
            Stroke = Accent,
            StrokeThickness = 1,
            StrokeDashArray = [3, 2],
            IsHitTestVisible = false,
        }, rect.X, rect.Y);
        if (handles && draft is { Free: true } && !sample)
        {
            foreach (var point in Handles(rect).Skip(1))
            {
                Add(new Shapes.Ellipse
                {
                    Width = 8,
                    Height = 8,
                    Fill = Mark,
                    Stroke = Accent,
                    StrokeThickness = 1.5,
                    IsHitTestVisible = false,
                }, point.X - 4, point.Y - 4);
            }
        }
        // Its X and Y above to its right, where the origin's are to the left.
        Dot(rect.TopLeft, Accent, Mark, 11);
        Callout($"{Number(place.X)}, {Number(place.Y)}", new Point(rect.X + 8, rect.Y - 24), Accent);
    }

    /// <summary>The eight handles of a rect: the corners and the middles of the sides, clockwise from the top-left.</summary>
    static Point[] Handles(Rect r) =>
    [
        r.TopLeft, new(r.X + r.Width / 2, r.Y), r.TopRight, new(r.Right, r.Y + r.Height / 2),
        r.BottomRight, new(r.X + r.Width / 2, r.Bottom), r.BottomLeft, new(r.X, r.Y + r.Height / 2),
    ];

    /// <summary>
    /// Sizes as on a drawing: the whole box's width above it and its height at
    /// its right, the room inside the padding under it, and each picked
    /// block's or group's width above it and height at its left.
    /// </summary>
    void DrawSizes(double inside, double height)
    {
        var outer = inside + 2 * Inset;
        var left = new Point(left0, top0);
        var right = new Point(left0 + outer * Zoom, top0);
        Across(left, right, -40, $"{Number(outer)}", Sizing);
        var bottom = top0 + height * Zoom;
        Down(new Point(right.X, top0), new Point(right.X, bottom), 22, $"{Number(height)}", Sizing);
        Across(new Point(OnStage(0, height).X, bottom), new Point(OnStage(inside, height).X, bottom), 36,
            Loc.Plain("editor.stage.inside", ("size", Number(inside))), Sizing);
        foreach (var unit in Units(chosen))
        {
            var place = UnitPlace(unit);
            if (place.IsEmpty)
            {
                continue;
            }
            var rect = StageRect(place);
            Across(rect.BottomLeft, rect.BottomRight, 14, Number(place.Width), Accent);
            Down(rect.TopLeft, rect.BottomLeft, -14, Number(place.Height), Accent);
        }
    }

    /// <summary>A width: a line `by` pixels above (less than 0) or below the edge from `from` to `to`, with ends and its number.</summary>
    void Across(Point from, Point to, double by, string text, Brush brush)
    {
        var y = from.Y + by;
        Line(from.X, from.Y, from.X, y + Math.Sign(by) * 4, brush, 0.6);
        Line(to.X, to.Y, to.X, y + Math.Sign(by) * 4, brush, 0.6);
        Line(from.X, y, to.X, y, brush, 1);
        Line(from.X, y - 4, from.X, y + 4, brush, 1);
        Line(to.X, y - 4, to.X, y + 4, brush, 1);
        Callout(text, new Point((from.X + to.X) / 2, y - 9), brush, center: true);
    }

    /// <summary>A height: a line `by` pixels right (more than 0) or left of the edge from `from` to `to`, with ends and its number.</summary>
    void Down(Point from, Point to, double by, string text, Brush brush)
    {
        var x = from.X + by;
        Line(from.X, from.Y, x + Math.Sign(by) * 4, from.Y, brush, 0.6);
        Line(to.X, to.Y, x + Math.Sign(by) * 4, to.Y, brush, 0.6);
        Line(x, from.Y, x, to.Y, brush, 1);
        Line(x - 4, from.Y, x + 4, from.Y, brush, 1);
        Line(x - 4, to.Y, x + 4, to.Y, brush, 1);
        var middle = (from.Y + to.Y) / 2 - 9;
        Callout(text, by > 0 ? new Point(x + 6, middle) : new Point(x - 6, middle), brush, center: false, before: by < 0);
    }

    void Line(double x1, double y1, double x2, double y2, Brush brush, double thickness) =>
        Add(new Shapes.Line { X1 = x1, Y1 = y1, X2 = x2, Y2 = y2, Stroke = brush, StrokeThickness = thickness, IsHitTestVisible = false }, 0, 0);

    /// <summary>A round point: `fill` inside, `ring` around, `size` across, its middle at `at`.</summary>
    void Dot(Point at, Brush fill, Brush ring, double size) =>
        Add(new Shapes.Ellipse { Width = size, Height = size, Fill = fill, Stroke = ring, StrokeThickness = 2, IsHitTestVisible = false },
            at.X - size / 2, at.Y - size / 2);

    /// <summary>A small label on a dark pill: its left at `at`, or its middle with `center`, or its right with `before`.</summary>
    void Callout(string text, Point at, Brush brush, bool center = false, bool before = false)
    {
        var label = new Border
        {
            Background = Pill,
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(4, 1, 4, 1),
            IsHitTestVisible = false,
            Child = new TextBlock { Text = text, FontSize = 11, Foreground = brush },
        };
        label.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
        var width = label.DesiredSize.Width;
        Add(label, center ? at.X - width / 2 : before ? at.X - width : at.X, at.Y);
    }

    void Add(UIElement element, double x, double y)
    {
        Canvas.SetLeft(element, x);
        Canvas.SetTop(element, y);
        Stage.Children.Add(element);
    }

    /// <summary>One item as the hook draws its kind: a text, a bar, a ring, a pie or an icon.</summary>
    FrameworkElement Draw(SceneItem item, BoxFrame box, double zoom)
    {
        double Scaled(int hundredths) => hundredths / 100.0 * zoom;
        var (w, h) = (Scaled(item.W), Scaled(item.H));
        var track = Brush(item.Track != 0 ? item.Track : box.Track);
        var fill = Math.Clamp(item.Fill, 0, 1000) / 1000.0;
        var grid = new Grid { Width = w, Height = h, IsHitTestVisible = false };
        switch (item.Kind)
        {
            case 1:
                var text = new TextBlock
                {
                    Text = item.Text,
                    FontFamily = SceneFont,
                    FontSize = Math.Max(1, Scaled(item.Size)),
                    FontWeight = FontWeight.FromOpenTypeWeight(Math.Clamp(item.Weight, 1, 999)),
                    Foreground = Brush(item.Color),
                    TextAlignment = item.Options switch { 1 => TextAlignment.Center, 2 => TextAlignment.Right, _ => TextAlignment.Left },
                    VerticalAlignment = VerticalAlignment.Center,
                    TextWrapping = TextWrapping.NoWrap,
                };
                grid.Children.Add(text);
                // Cut only a text wider than its box, as the taskbar does; a
                // glyph reaching a little past its own width, as a "%" does,
                // is not.
                text.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
                grid.ClipToBounds = text.DesiredSize.Width > w + 0.5 * zoom;
                break;
            case 2:
                var radius = new CornerRadius(h / 2);
                grid.Children.Add(new Border { Background = track, CornerRadius = radius });
                if (item.Fill >= 0)
                {
                    grid.Children.Add(new Border
                    {
                        Background = Brush(item.Color),
                        CornerRadius = radius,
                        Width = w * fill,
                        HorizontalAlignment = item.Options == 1 ? HorizontalAlignment.Right : HorizontalAlignment.Left,
                    });
                }
                break;
            case 3:
            case 4:
                var side = Math.Min(w, h);
                var pie = item.Kind == 4;
                var thickness = pie ? side / 2 : Math.Min(Scaled(item.Size), side / 2);
                grid.Children.Add(Round(side, pie, thickness, track));
                if (item.Fill >= 0 && fill > 0)
                {
                    grid.Children.Add(Arc(new Point(w / 2, h / 2), side, fill, item.Options == 1, pie, thickness, Brush(item.Color)));
                }
                break;
            case 5:
                var (id, color) = icons.TryGetValue(item.Slot, out var icon) ? icon : ("?", 0xFFFFFFFFu);
                var named = ServiceName(id);
                grid.Children.Add(new Shapes.Ellipse { Fill = Brush(color) });
                grid.Children.Add(new TextBlock
                {
                    Text = named.Length > 0 ? named[..1].ToUpperInvariant() : "?",
                    FontSize = Math.Max(1, h * 0.55),
                    FontWeight = FontWeights.SemiBold,
                    Foreground = Brushes.Black,
                    HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center,
                });
                break;
        }
        return grid;
    }

    /// <summary>
    /// The colored part of a ring or pie: `share` of the turn from twelve
    /// o'clock clockwise, or ending there when it sits against the end.
    /// </summary>
    static UIElement Arc(Point center, double side, double share, bool atEnd, bool pie, double thickness, Brush brush)
    {
        if (share >= 1)
        {
            return Round(side, pie, thickness, brush);
        }
        // A stroke is drawn on both sides of its line, so a ring's line runs
        // half its thickness in from the edge.
        var r = pie ? side / 2 : (side - thickness) / 2;
        var sweep = share * 360;
        var from = atEnd ? 360 - sweep : 0;
        Point At(double degrees)
        {
            var radians = degrees * Math.PI / 180;
            return new Point(center.X + r * Math.Sin(radians), center.Y - r * Math.Cos(radians));
        }
        var figure = new PathFigure { StartPoint = pie ? center : At(from), IsClosed = pie, IsFilled = pie };
        if (pie)
        {
            figure.Segments.Add(new LineSegment(At(from), false));
        }
        figure.Segments.Add(new ArcSegment(At(from + sweep), new Size(r, r), 0, sweep > 180, SweepDirection.Clockwise, true));
        var path = new Shapes.Path { Data = new PathGeometry([figure]) };
        if (pie)
        {
            path.Fill = brush;
        }
        else
        {
            path.Stroke = brush;
            path.StrokeThickness = thickness;
            // Round ends, as the hook draws a ring's arc.
            path.StrokeStartLineCap = path.StrokeEndLineCap = PenLineCap.Round;
        }
        return path;
    }

    /// <summary>A whole disc, or a whole ring of this thickness, `side` across.</summary>
    static Shapes.Ellipse Round(double side, bool pie, double thickness, Brush brush) => pie
        ? new Shapes.Ellipse { Width = side, Height = side, Fill = brush }
        : new Shapes.Ellipse { Width = side, Height = side, Stroke = brush, StrokeThickness = thickness };
}
