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

// The box editor's mouse and keys on the stage, and arranging a free box's blocks.
public partial class EditorWindow
{
    // ---- The mouse and the keys on the stage.

    /// <summary>The block under a point, the last drawn first, as the one on top.</summary>
    int? BlockAt(Point stage)
    {
        if (draft is null)
        {
            return null;
        }
        for (var at = draft.Blocks.Count - 1; at >= 0; at--)
        {
            var place = Place(at);
            if (!place.IsEmpty && StageRect(place) is var rect && rect.Contains(stage))
            {
                return at;
            }
        }
        return null;
    }

    /// <summary>The handle of the block or group picked alone under a point, if any; its point is the top-left one.</summary>
    int? HandleAt(Point stage)
    {
        if (draft is not { Free: true } || sample || Units(chosen) is not [var unit])
        {
            return null;
        }
        var handles = Handles(StageRect(UnitPlace(unit)));
        for (var at = 0; at < handles.Length; at++)
        {
            if (Math.Abs(handles[at].X - stage.X) <= 6 && Math.Abs(handles[at].Y - stage.Y) <= 6)
            {
                return at;
            }
        }
        return null;
    }

    /// <summary>
    /// The pointer over the stage: the scrolling arrows while Space is held
    /// to move the view, arrows for a handle, and for a block a free box lets
    /// move.
    /// </summary>
    Cursor? CursorAt(Point stage) => spaceHeld ? Cursors.ScrollAll : HandleAt(stage) switch
    {
        0 or 4 => Cursors.SizeNWSE,
        2 or 6 => Cursors.SizeNESW,
        1 or 5 => Cursors.SizeNS,
        3 or 7 => Cursors.SizeWE,
        _ => draft is { Free: true } && !sample && BlockAt(stage) is not null ? Cursors.SizeAll : null,
    };

    void StagePressed(object sender, MouseButtonEventArgs e)
    {
        Stage.Focus();
        if (draft is null)
        {
            return;
        }
        var at = e.GetPosition(Stage);
        pressAt = at;
        moved = false;
        startWidth = draft.Width;
        var free = draft.Free && !sample;
        if (free && HandleAt(at) is { } grabbed && Units(chosen) is [var sizing])
        {
            gesture = Gesture.Resize;
            handle = grabbed;
            resizing = sizing;
            startRect = UnitPlace(sizing);
            startSizes = sizing.ToDictionary(i => i, i => (Place(i), draft.Blocks[i].Size, draft.Blocks[i].Thickness));
            Stage.CaptureMouse();
            return;
        }
        var adding = (Keyboard.Modifiers & (ModifierKeys.Control | ModifierKeys.Shift)) != 0;
        letGo = null;
        enterAt = null;
        if (BlockAt(at) is { } hit)
        {
            // Shift or Ctrl and a press add to the pick, and the press still
            // drags all that is picked; a block picked already is let go when
            // the press does not move it.
            var unit = Units([hit])[0];
            var picked = unit.All(chosen.Contains);
            // A group picked alone and pressed again at one of its blocks: a
            // click that does not move picks that block alone.
            if (!adding && picked && unit.Count > 1 && Units(chosen).Count == 1)
            {
                enterAt = hit;
            }
            if (!picked)
            {
                Choose(adding ? chosen.Union(unit) : [hit], hit);
            }
            else if (primary != hit)
            {
                primary = hit;
                ListBlocks();
                FillBlock();
            }
            if (adding && picked)
            {
                letGo = unit;
                if (!free)
                {
                    LetGo();
                    return;
                }
            }
            if (free)
            {
                gesture = Gesture.Move;
                startPlaces = chosen.ToDictionary(i => i, i => new Point(draft.Blocks[i].X, draft.Blocks[i].Y));
                startRect = UnitPlace(chosen);
                Stage.CaptureMouse();
            }
            DrawStage();
            return;
        }
        if (!adding)
        {
            Choose([], -1);
        }
        gesture = Gesture.Marquee;
        marquee = new Rect(at, at);
        Stage.CaptureMouse();
    }

    void StageMoved(object sender, MouseEventArgs e)
    {
        var at = e.GetPosition(Stage);
        if (gesture == Gesture.None || draft is null)
        {
            Stage.Cursor = CursorAt(at);
            return;
        }
        var (rawX, rawY) = ((at.X - pressAt.X) / Zoom, (at.Y - pressAt.Y) / Zoom);
        var (dx, dy) = (Math.Round(rawX), Math.Round(rawY));
        switch (gesture)
        {
            case Gesture.Move or Gesture.Resize:
                if (!moved && dx == 0 && dy == 0)
                {
                    return;
                }
                if (!moved)
                {
                    Checkpoint();
                    moved = true;
                }
                if (gesture == Gesture.Move)
                {
                    // Shift keeps it straight across or down, the way it has
                    // moved more, as on a slide.
                    if ((Keyboard.Modifiers & ModifierKeys.Shift) != 0)
                    {
                        if (Math.Abs(rawX) >= Math.Abs(rawY))
                        {
                            dy = 0;
                        }
                        else
                        {
                            dx = 0;
                        }
                    }
                    Shift(startPlaces, startRect, startWidth, dx, dy);
                    break;
                }
                // Shift keeps the block's shape, as on a slide.
                var rect = (Keyboard.Modifiers & ModifierKeys.Shift) != 0
                    ? Scaled(startRect, handle, rawX, rawY)
                    : Stretched(startRect, handle, dx, dy);
                if (resizing is [var only])
                {
                    var sized = draft.Blocks[only];
                    (sized.X, sized.Y, sized.W, sized.H) = (rect.X, rect.Y, rect.Width, rect.Height);
                }
                else
                {
                    Rescale(rect);
                }
                draft.Width = Math.Min(Widest, Math.Max(startWidth, Math.Round(rect.Right, 2)));
                ShowWidth();
                break;
            case Gesture.Marquee:
                marquee = new Rect(pressAt, at);
                break;
        }
        DrawStage();
    }

    void StageReleased(object sender, MouseButtonEventArgs e)
    {
        var was = gesture;
        gesture = Gesture.None;
        Stage.ReleaseMouseCapture();
        if (draft is null)
        {
            return;
        }
        switch (was)
        {
            case Gesture.Move or Gesture.Resize when moved:
                moved = false;
                letGo = null;
                enterAt = null;
                FillBlock();
                Touch();
                return;
            case Gesture.Move when letGo is not null:
                LetGo();
                return;
            case Gesture.Move when enterAt is { } block:
                Enter(block);
                return;
            case Gesture.Marquee:
                var caught = Enumerable.Range(0, draft.Blocks.Count)
                    .Where(i => Place(i) is { IsEmpty: false } place && StageRect(place).IntersectsWith(marquee))
                    .ToList();
                Choose(chosen.Concat(caught), caught.DefaultIfEmpty(primary).Last());
                return;
        }
        DrawStage();
    }

    /// <summary>
    /// A group sized as one, to `to` from where it started: each block's place
    /// and size by the same factors from the group's corner, a text's size by
    /// the height's factor (8 to 24 DIPs), and a ring's own thickness by the
    /// smaller factor.
    /// </summary>
    void Rescale(Rect to)
    {
        if (draft is null || startRect.Width <= 0 || startRect.Height <= 0)
        {
            return;
        }
        var (sx, sy) = (to.Width / startRect.Width, to.Height / startRect.Height);
        foreach (var (at, (place, size, thickness)) in startSizes)
        {
            var block = draft.Blocks[at];
            block.X = Math.Max(0, Math.Round(to.X + (place.X - startRect.X) * sx, 1));
            block.Y = Math.Max(0, Math.Round(to.Y + (place.Y - startRect.Y) * sy, 1));
            block.W = Math.Max(1, Math.Round(place.Width * sx, 1));
            block.H = Math.Max(1, Math.Round(place.Height * sy, 1));
            if (block.Kind == "text")
            {
                block.Size = sy == 1 ? size : Within(Math.Round((size ?? 11) * sy * 2) / 2, 8, 24);
            }
            if (thickness is { } t && block.Kind is "ring" or "rings")
            {
                block.Thickness = Math.Max(1, Math.Round(t * Math.Min(sx, sy) * 2) / 2);
            }
        }
    }

    /// <summary>Takes out of the pick the blocks a Shift or Ctrl click let go.</summary>
    void LetGo()
    {
        var rest = chosen.Except(letGo ?? []).ToList();
        letGo = null;
        Choose(rest, rest.Contains(primary) ? primary : rest.DefaultIfEmpty(-1).Max());
    }

    /// <summary>
    /// The keys: Ctrl+S saves and Ctrl+N makes a new preset anywhere; Enter
    /// takes a typed place; Ctrl+Z and Ctrl+Y undo and redo, Ctrl+G and
    /// Ctrl+Shift+G group and ungroup; and on the stage or the blocks' list
    /// Ctrl+A picks all, Delete removes, Escape lets go and the arrows move a
    /// free box's picked blocks. Over a new preset's card, Enter makes it and
    /// Escape does not.
    /// </summary>
    void Keyed(object sender, KeyEventArgs e)
    {
        var control = (Keyboard.Modifiers & ModifierKeys.Control) != 0;
        var shift = (Keyboard.Modifiers & ModifierKeys.Shift) != 0;
        if (CreateLayer.Visibility == Visibility.Visible)
        {
            if (e.Key == Key.Escape)
            {
                CreateCancelled(this, e);
                e.Handled = true;
            }
            else if (e.Key == Key.Enter)
            {
                CreateConfirmed(this, e);
                e.Handled = true;
            }
            return;
        }
        switch (e.Key)
        {
            case Key.S when control:
                SaveBox(this, e);
                e.Handled = true;
                return;
            case Key.N when control:
                CreateClicked(this, e);
                e.Handled = true;
                return;
            case Key.Enter when Keyboard.FocusedElement is TextBox typed:
                Committed(typed);
                return;
        }
        if (draft is null || Keyboard.FocusedElement is TextBox)
        {
            return;
        }
        // Held over the stage, Space makes the left button move the view, as
        // in drawing tools, and does not press what has the focus. It goes
        // with the mouse, so it looks at the pointer, not the focus.
        if (e.Key == Key.Space && (spaceHeld || StageView.IsMouseOver))
        {
            e.Handled = true;
            if (!spaceHeld)
            {
                spaceHeld = true;
                ShowPanning();
            }
            return;
        }
        var onBlocks = Stage.IsKeyboardFocused || BlockList.IsKeyboardFocusWithin;
        var handled = true;
        switch (e.Key)
        {
            case Key.Z when control:
                Step(undo, redo);
                break;
            case Key.Y when control:
                Step(redo, undo);
                break;
            case Key.G when control && shift:
                UngroupClicked(this, e);
                break;
            case Key.G when control:
                GroupClicked(this, e);
                break;
            case Key.A when control && onBlocks:
                Choose(Enumerable.Range(0, draft.Blocks.Count), primary);
                break;
            case Key.Delete when onBlocks:
                RemoveBlocks(this, e);
                break;
            case Key.Escape when onBlocks:
                Choose([], -1);
                break;
            case Key.Left or Key.Right or Key.Up or Key.Down when Stage.IsKeyboardFocused && draft.Free && !sample && chosen.Count > 0:
                var step = shift ? 5 : 1;
                var (dx, dy) = e.Key switch { Key.Left => (-step, 0), Key.Right => (step, 0), Key.Up => (0, -step), _ => (0, step) };
                Checkpoint(typing: true);
                Shift(chosen.ToDictionary(i => i, i => new Point(draft.Blocks[i].X, draft.Blocks[i].Y)), UnitPlace(chosen), draft.Width, dx, dy);
                FillBlock();
                Touch();
                break;
            default:
                handled = false;
                break;
        }
        e.Handled = handled;
    }

    // ---- Arranging a free box's blocks, as on a slide.

    /// <summary>
    /// Aligns the picked blocks, groups as one: to the edges or middles of
    /// what is picked, or with one picked to the box's; or spaces three or
    /// more evenly across or down.
    /// </summary>
    void AlignClicked(object sender, RoutedEventArgs e)
    {
        if (draft is not { Free: true } || sample || sender is not FrameworkElement { Tag: string how })
        {
            return;
        }
        var units = Units(chosen).Select(u => (Blocks: u, Place: UnitPlace(u))).Where(u => !u.Place.IsEmpty).ToList();
        if (units.Count == 0 || (how is "across" or "down" && units.Count < 3))
        {
            return;
        }
        var target = Rect.Empty;
        foreach (var unit in units)
        {
            target.Union(unit.Place);
        }
        if (units.Count == 1)
        {
            target = new Rect(0, 0, draft.Width, Tall);
        }
        Checkpoint();
        var shifts = new List<(List<int> Blocks, double Dx, double Dy)>();
        if (how is "across" or "down")
        {
            var across = how == "across";
            var sorted = units.OrderBy(u => across ? u.Place.Left : u.Place.Top).ToList();
            var taken = sorted.Sum(u => across ? u.Place.Width : u.Place.Height);
            var gap = ((across ? target.Width : target.Height) - taken) / (sorted.Count - 1);
            var next = across ? target.Left : target.Top;
            foreach (var unit in sorted)
            {
                var d = next - (across ? unit.Place.Left : unit.Place.Top);
                shifts.Add((unit.Blocks, across ? d : 0, across ? 0 : d));
                next += (across ? unit.Place.Width : unit.Place.Height) + gap;
            }
        }
        else
        {
            foreach (var (blocks, place) in units)
            {
                var dx = how switch
                {
                    "left" => target.Left - place.Left,
                    "center" => target.Left + target.Width / 2 - (place.Left + place.Width / 2),
                    "right" => target.Right - place.Right,
                    _ => 0,
                };
                var dy = how switch
                {
                    "top" => target.Top - place.Top,
                    "middle" => target.Top + target.Height / 2 - (place.Top + place.Height / 2),
                    "bottom" => target.Bottom - place.Bottom,
                    _ => 0,
                };
                shifts.Add((blocks, dx, dy));
            }
        }
        foreach (var (blocks, dx, dy) in shifts)
        {
            foreach (var at in blocks)
            {
                draft.Blocks[at].X = Math.Max(0, Math.Round(draft.Blocks[at].X + dx, 1));
                draft.Blocks[at].Y = Math.Max(0, Math.Round(draft.Blocks[at].Y + dy, 1));
            }
        }
        FillBlock();
        Touch();
    }

    /// <summary>Groups the picked blocks, which then are picked, moved and aligned as one.</summary>
    void GroupClicked(object sender, RoutedEventArgs e)
    {
        if (draft is not { Free: true } || sample || Units(chosen).Count < 2)
        {
            return;
        }
        Checkpoint();
        into = null;
        var group = draft.Blocks.Select(b => b.Group ?? 0).DefaultIfEmpty(0).Max() + 1;
        foreach (var at in chosen)
        {
            draft.Blocks[at].Group = group;
        }
        Touch();
    }

    void UngroupClicked(object sender, RoutedEventArgs e)
    {
        if (draft is null || sample || !chosen.Any(i => draft.Blocks[i].Group is not null))
        {
            return;
        }
        Checkpoint();
        foreach (var at in chosen)
        {
            draft.Blocks[at].Group = null;
        }
        Touch();
    }
}
