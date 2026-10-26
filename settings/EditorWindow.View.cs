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

// The box editor's view of the stage: the zoom, Ctrl and the wheel, and moving the view by dragging and scrolling.
public partial class EditorWindow
{
    void ViewChanged(object sender, RoutedEventArgs e)
    {
        if (loading)
        {
            return;
        }
        if (sender == ZoomBox)
        {
            if (ZoomBox.SelectedValue is not string picked)
            {
                return;
            }
            Zoom = double.Parse(picked, CultureInfo.InvariantCulture);
            centering = true;
            // A zoom the wheel left in the list goes once the list has closed.
            Dispatcher.InvokeAsync(ShowZoom, DispatcherPriority.Background);
        }
        DrawStage();
    }

    /// <summary>
    /// Shows the zoom in the zoom box: its 2×, 3× and 4×, and in its place
    /// among them a zoom the wheel came to.
    /// </summary>
    void ShowZoom()
    {
        static string Key(double zoom) => zoom.ToString("0.##", CultureInfo.InvariantCulture);
        var listed = ZoomPicks.Contains(Zoom) ? ZoomPicks : [.. ZoomPicks.Append(Zoom).Order()];
        var was = loading;
        loading = true;
        ZoomBox.ItemsSource = listed.Select(zoom => new Option(Key(zoom), $"{Key(zoom)}×")).ToArray();
        ZoomBox.SelectedValue = Key(Zoom);
        loading = was;
    }

    /// <summary>
    /// Ctrl and the wheel over the stage zoom a step of <see cref="Zooms"/> at
    /// a time, keeping the point of the box under the pointer where it is on
    /// the screen. The wheel alone moves the view up and down,
    /// and with Shift across.
    /// </summary>
    void StageWheeled(object sender, MouseWheelEventArgs e)
    {
        if (frame is null || draft is null)
        {
            return;
        }
        e.Handled = true;
        // A drag moves its blocks by how far the pointer went from where it
        // pressed on the stage, so a box zoomed or moved on the way would
        // make them jump.
        if (gesture != Gesture.None)
        {
            return;
        }
        var modifiers = Keyboard.Modifiers;
        if ((modifiers & ModifierKeys.Control) == 0)
        {
            var by = e.Delta / (double)Mouse.MouseWheelDeltaForOneLine * WheelStep();
            var across = (modifiers & ModifierKeys.Shift) != 0;
            Pan(BoxShown(), across ? by : 0, across ? 0 : by);
            return;
        }
        if (Math.Sign(wheelRest) == -Math.Sign(e.Delta))
        {
            wheelRest = 0;
        }
        wheelRest += e.Delta;
        var steps = wheelRest / Mouse.MouseWheelDeltaForOneLine;
        wheelRest %= Mouse.MouseWheelDeltaForOneLine;
        var at = Math.Max(0, Array.FindLastIndex(Zooms, zoom => zoom <= Zoom));
        var to = Zooms[Math.Clamp(at + steps, 0, Zooms.Length - 1)];
        if (to == Zoom)
        {
            return;
        }
        // The point under the pointer, in DIPs from the box's outer top-left
        // corner, and where the pointer is in what the stage shows.
        var stage = e.GetPosition(Stage);
        var (x, y) = ((stage.X - left0) / Zoom, (stage.Y - top0) / Zoom);
        var (shownX, shownY) = (stage.X - StageView.HorizontalOffset, stage.Y - StageView.VerticalOffset);
        Zoom = to;
        ShowZoom();
        // That point goes back under the pointer.
        PlaceBox(shownX - x * Zoom, shownY - y * Zoom);
    }

    /// <summary>How far a notch of the wheel moves the view, in pixels: as far as the stage scrolled before.</summary>
    double WheelStep() =>
        SystemParameters.WheelScrollLines is > 0 and var lines ? lines * 16.0 : StageView.ViewportHeight;

    /// <summary>Where the box's outer top-left corner shows in the stage's view, in pixels.</summary>
    Point BoxShown() => new(left0 - StageView.HorizontalOffset, top0 - StageView.VerticalOffset);

    /// <summary>
    /// Puts the box's outer top-left corner at a point of the stage's view:
    /// the box moves on the stage as far as the stage's room on the left and
    /// top lets it, and the stage scrolls for the rest.
    /// </summary>
    void PlaceBox(double x, double y)
    {
        left0 = Math.Round(x);
        top0 = Math.Round(y);
        var (scrollX, scrollY) = (Math.Max(0, Room - left0), Math.Max(0, Room - top0));
        left0 += scrollX;
        top0 += scrollY;
        // The whole of what shows is stage, scroll bars or not.
        spanX = scrollX > 0 ? scrollX + StageView.ActualWidth : 0;
        spanY = scrollY > 0 ? scrollY + StageView.ActualHeight : 0;
        centering = false;
        DrawStage();
        StageView.ScrollToHorizontalOffset(scrollX);
        StageView.ScrollToVerticalOffset(scrollY);
    }

    /// <summary>
    /// Moves the view so the box shows moved by (dx, dy) pixels from where it
    /// showed from. The box never leaves the view: where it fits, it keeps
    /// the stage's room to the view's edges; where it does not, it goes as
    /// far as its own edges show with that room, as far as the stage
    /// scrolls. A box the zoom left farther out only comes back.
    /// </summary>
    void Pan(Point from, double dx, double dy)
    {
        if (frame is not { } box)
        {
            return;
        }
        var reach = Reach(box);
        static double Along(double at, double by, double size, double shown)
        {
            var (low, high) = (Math.Min(Room, shown - Room - size), Math.Max(Room, shown - Room - size));
            return Math.Clamp(at + by, Math.Min(low, at), Math.Max(high, at));
        }
        PlaceBox(Along(from.X, dx, reach.Width * Zoom, StageView.ViewportWidth),
            Along(from.Y, dy, reach.Height * Zoom, StageView.ViewportHeight));
    }

    /// <summary>
    /// How far the stage holds the box, in DIPs from its outer top-left
    /// corner: the box, and a free box's blocks already outside it, so they
    /// can be picked and brought in.
    /// </summary>
    Size Reach(BoxFrame box)
    {
        // A free box is as wide as the draft says, even while a block pushes it.
        var reach = new Size((draft!.Free ? draft.Width : box.Width / 100.0) + 2 * Inset, box.Height / 100.0);
        if (draft.Free)
        {
            for (var at = 0; at < draft.Blocks.Count; at++)
            {
                var place = Place(at);
                reach.Width = Math.Max(reach.Width, Inset + place.Right);
                reach.Height = Math.Max(reach.Height, place.Bottom);
            }
        }
        return reach;
    }

    /// <summary>
    /// The middle button, or the left with Space held, pressed over the stage
    /// but not its scroll bars, starts moving the view: the point of the box
    /// pressed follows the pointer.
    /// </summary>
    void ViewPressed(object sender, MouseButtonEventArgs e)
    {
        var panning = e.ChangedButton == MouseButton.Middle
            || (e.ChangedButton == MouseButton.Left && spaceHeld && Keyboard.IsKeyDown(Key.Space));
        if (!panning || frame is null || draft is null || gesture != Gesture.None || OnScrollBar(e.OriginalSource))
        {
            return;
        }
        e.Handled = true;
        gesture = Gesture.Pan;
        panFrom = BoxShown();
        panAt = e.GetPosition(StageView);
        StageView.CaptureMouse();
        ShowPanning();
    }

    void ViewMoved(object sender, MouseEventArgs e)
    {
        if (gesture != Gesture.Pan)
        {
            return;
        }
        e.Handled = true;
        var at = e.GetPosition(StageView);
        // From where it began, so a move held back at the edge does not
        // leave the box behind the pointer.
        Pan(panFrom, at.X - panAt.X, at.Y - panAt.Y);
    }

    /// <summary>Whether a press landed on one of the stage's scroll bars.</summary>
    bool OnScrollBar(object source)
    {
        for (var at = source as DependencyObject; at is not null && at != StageView;
            at = at is Visual ? VisualTreeHelper.GetParent(at) : LogicalTreeHelper.GetParent(at))
        {
            if (at is ScrollBar)
            {
                return true;
            }
        }
        return false;
    }

    /// <summary>The scrolling arrows over the stage while the view moves or Space is held to move it.</summary>
    void ShowPanning()
    {
        var panning = spaceHeld || gesture == Gesture.Pan;
        StageView.Cursor = panning ? Cursors.ScrollAll : null;
        if (gesture == Gesture.None)
        {
            Stage.Cursor = CursorAt(Mouse.GetPosition(Stage));
        }
    }

    void LetGoOfSpace()
    {
        if (spaceHeld)
        {
            spaceHeld = false;
            ShowPanning();
        }
    }

    /// <summary>
    /// The wheel tilted, or a touchpad swiped across, over the stage moves
    /// the view across: WPF passes on only the upright wheel.
    /// </summary>
    IntPtr Hooked(IntPtr hwnd, int message, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        const int WM_MOUSEHWHEEL = 0x020E;
        if (message == WM_MOUSEHWHEEL && StageView.IsMouseOver && frame is not null && draft is not null && gesture == Gesture.None)
        {
            // Turned right shows more of the right, which moves the box left.
            var delta = (short)((wParam.ToInt64() >> 16) & 0xFFFF);
            Pan(BoxShown(), -delta / (double)Mouse.MouseWheelDeltaForOneLine * WheelStep(), 0);
            handled = true;
        }
        return IntPtr.Zero;
    }
}
