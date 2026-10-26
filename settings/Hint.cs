using System.Globalization;
using System.Runtime.CompilerServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;

namespace LimitPeeker.Settings;

/// <summary>
/// A faint hint in an empty text box: what it takes, or what is used when it
/// is left empty. It is drawn over the box, so the theme's box and its clear
/// button stay as they are, and goes once anything is typed.
/// </summary>
internal sealed class Hint : Adorner
{
    static readonly ConditionalWeakTable<TextBox, Hint> Hints = new();
    string text;
    bool placed;

    Hint(TextBox box, string text) : base(box)
    {
        this.text = text;
        IsHitTestVisible = false;
        box.TextChanged += (_, _) => InvalidateVisual();
        box.IsVisibleChanged += (_, _) => InvalidateVisual();
    }

    /// <summary>Gives a box its hint, or another one; an empty hint shows nothing.</summary>
    internal static void On(TextBox box, string text)
    {
        if (Hints.TryGetValue(box, out var hint))
        {
            hint.text = text;
            hint.InvalidateVisual();
            return;
        }
        hint = new Hint(box, text);
        Hints.Add(box, hint);
        if (box.IsLoaded)
        {
            hint.Place();
        }
        else
        {
            box.Loaded += (_, _) => hint.Place();
        }
    }

    void Place()
    {
        if (!placed && AdornerLayer.GetAdornerLayer(AdornedElement) is { } layer)
        {
            layer.Add(this);
            placed = true;
        }
    }

    protected override void OnRender(DrawingContext context)
    {
        var box = (TextBox)AdornedElement;
        if (text.Length == 0 || box.Text.Length > 0 || !box.IsVisible)
        {
            return;
        }
        var brush = box.TryFindResource("TextFillColorTertiaryBrush") as Brush ?? Brushes.Gray;
        var shown = new FormattedText(text, CultureInfo.CurrentUICulture, box.FlowDirection,
            new Typeface(box.FontFamily, box.FontStyle, box.FontWeight, box.FontStretch), box.FontSize, brush,
            VisualTreeHelper.GetDpi(this).PixelsPerDip);
        // Where typed text starts: inside the border and padding, past the caret's room.
        var left = box.BorderThickness.Left + box.Padding.Left + 2;
        context.DrawText(shown, new Point(left, (box.ActualHeight - shown.Height) / 2));
    }
}
