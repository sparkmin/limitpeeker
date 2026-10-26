using System.Windows;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;

namespace LimitPeeker.Settings;

/// <summary>
/// The taskbar box's own background color on the editor's Box line: the same
/// swatch, eyedropper, opacity and Default as the Boxes page, sending the same
/// command, so what changes is the chosen box's color and the preset's file
/// holds none. The stage shows the preset on that color, asked of the
/// controller anew when the color changes. The eyedropper also fills a
/// block's color field.
/// </summary>
public partial class EditorWindow
{
    /// <summary>What the eyedropper is taking a color for, while it has the pointer.</summary>
    enum Picking { None, Box, Block }

    uint boxColor = 0xFF1F2227;
    bool boxColorDefault = true;
    // The color last shown, so a change asks for the preview again.
    string shownColor = "";
    // What the eyedropper is for, and whether the opacity's thumb is being
    // dragged, which sends once it is let go.
    Picking picking;
    bool draggingOpacity;

    void WireBoxColor()
    {
        BoxOpacitySlider.AddHandler(Thumb.DragStartedEvent, new DragStartedEventHandler((_, _) => draggingOpacity = true));
        BoxOpacitySlider.AddHandler(Thumb.DragCompletedEvent, new DragCompletedEventHandler((_, _) =>
        {
            draggingOpacity = false;
            SendBoxOpacity();
        }));
    }

    /// <summary>
    /// Shows the chosen box's color from the state, and asks for the preview
    /// again when it changed, as the stage draws the preset on it.
    /// </summary>
    void KnownColor(State state)
    {
        if (picking != Picking.Box)
        {
            ShowBoxColor(state.Color);
        }
        if (shownColor != state.Color)
        {
            shownColor = state.Color;
            if (draft is not null)
            {
                SendDraft();
            }
        }
    }

    void ShowBoxColor(string color)
    {
        boxColorDefault = color == "default";
        boxColor = (boxColorDefault ? null : MainWindow.ParseColor(color)) ?? MainWindow.ParseColor(MainWindow.DefaultColor) ?? 0;
        BoxSwatch.Background = new SolidColorBrush(MainWindow.ToColor(boxColor));
        loading = true;
        try
        {
            BoxOpacitySlider.Value = Math.Round((boxColor >> 24) * 100.0 / 255);
        }
        finally
        {
            loading = false;
        }
        BoxOpacityText.Text = $"{(int)Math.Round(BoxOpacitySlider.Value)}%";
        BoxDefaultButton.IsEnabled = !boxColorDefault;
    }

    /// <summary>The opacity a color picked keeps: the box's, or opaque when the box has no background.</summary>
    uint KeptAlpha => boxColor >> 24 == 0 ? 0xFF000000u : boxColor & 0xFF000000u;

    void SendBoxColor(uint argb) => send($"color\t{MainWindow.Hex(argb)}");

    void BoxColorClicked(object sender, RoutedEventArgs e)
    {
        if (Picker.Choose(new WindowInteropHelper(this).Handle, boxColor) is { } rgb)
        {
            SendBoxColor(KeptAlpha | rgb & 0x00FFFFFF);
        }
    }

    void BoxDropperClicked(object sender, RoutedEventArgs e) => StartPicking(Picking.Box, BoxDropperButton);

    /// <summary>The eyedropper for the picked block's color: the field takes the color as #RRGGBB.</summary>
    void BlockDropperClicked(object sender, RoutedEventArgs e) => StartPicking(Picking.Block, BlockDropperButton);

    void StartPicking(Picking what, UIElement from)
    {
        picking = what;
        Mouse.OverrideCursor = Cursors.Cross;
        Keyboard.Focus(from);
        Mouse.Capture(this, CaptureMode.Element);
    }

    /// <summary>Shows the color under the pointer where it would go.</summary>
    void PreviewPick(uint rgb)
    {
        switch (picking)
        {
            case Picking.Box:
                BoxSwatch.Background = new SolidColorBrush(MainWindow.ToColor(KeptAlpha | rgb & 0x00FFFFFF));
                break;
            case Picking.Block:
                ColorSwatch.Background = new SolidColorBrush(MainWindow.ToColor(0xFF000000u | rgb & 0x00FFFFFF));
                break;
        }
    }

    uint? PixelUnder(MouseEventArgs e)
    {
        var at = PointToScreen(e.GetPosition(this));
        return Picker.PixelAt((int)Math.Round(at.X), (int)Math.Round(at.Y));
    }

    void EndPicking(uint? rgb)
    {
        var what = picking;
        if (what == Picking.None)
        {
            return;
        }
        picking = Picking.None;
        Mouse.OverrideCursor = null;
        if (Mouse.Captured == this)
        {
            ReleaseMouseCapture();
        }
        switch (what, rgb)
        {
            case (Picking.Box, { } picked):
                SendBoxColor(KeptAlpha | picked & 0x00FFFFFF);
                break;
            case (Picking.Box, null):
                if (known is { } state)
                {
                    ShowBoxColor(state.Color);
                }
                break;
            // Through the field, so the block changes as when it is typed.
            case (Picking.Block, { } picked):
                ColorText.Text = $"#{picked & 0x00FFFFFF:X6}";
                break;
            case (Picking.Block, null):
                ColorSwatch.Background = ParseColor(ColorText.Text) is { } color ? new SolidColorBrush(color) : Brushes.Transparent;
                break;
        }
    }

    protected override void OnPreviewMouseMove(MouseEventArgs e)
    {
        if (picking != Picking.None)
        {
            if (PixelUnder(e) is { } rgb)
            {
                PreviewPick(rgb);
            }
            e.Handled = true;
            return;
        }
        base.OnPreviewMouseMove(e);
    }

    protected override void OnPreviewMouseDown(MouseButtonEventArgs e)
    {
        if (picking != Picking.None)
        {
            EndPicking(e.ChangedButton == MouseButton.Left ? PixelUnder(e) : null);
            e.Handled = true;
            return;
        }
        base.OnPreviewMouseDown(e);
    }

    protected override void OnPreviewKeyDown(KeyEventArgs e)
    {
        if (picking != Picking.None)
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

    void BoxDefaultClicked(object sender, RoutedEventArgs e) => send("color\tdefault");

    void BoxOpacityChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        BoxOpacityText.Text = $"{(int)Math.Round(BoxOpacitySlider.Value)}%";
        if (!loading && !draggingOpacity)
        {
            SendBoxOpacity();
        }
    }

    void SendBoxOpacity()
    {
        var alpha = (uint)Math.Round(BoxOpacitySlider.Value * 255 / 100);
        var argb = alpha << 24 | boxColor & 0x00FFFFFF;
        if (argb != boxColor)
        {
            SendBoxColor(argb);
        }
    }
}
