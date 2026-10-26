using System.Runtime.InteropServices;

namespace LimitPeeker.Settings;

/// <summary>
/// The window's frame. It has no maximize button: hovering one makes Explorer
/// start its Snap layouts thread, and LimitPeeker cannot attach to Explorer
/// while that thread exists. The window can still be resized.
/// </summary>
internal static class Frame
{
    const int Style = -16;
    const nint MaximizeBox = 0x00010000;

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    static extern nint GetWindowLongPtr(nint window, int index);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    static extern nint SetWindowLongPtr(nint window, int index, nint value);

    internal static void WithoutMaximize(nint window) =>
        SetWindowLongPtr(window, Style, GetWindowLongPtr(window, Style) & ~MaximizeBox);
}
