using System.Runtime.InteropServices;

namespace LimitPeeker.Settings;

/// <summary>
/// Windows' color dialog, and the pixel under a point on the screen, for a
/// box's color: comdlg32 and GDI called straight, so the window needs no
/// Windows Forms. The dialog's custom colors are kept for the run.
/// </summary>
internal static class Picker
{
    [StructLayout(LayoutKind.Sequential)]
    struct ChooseColorData
    {
        public int Size;
        public nint Owner;
        public nint Instance;
        public uint Result;
        public nint CustomColors;
        public uint Flags;
        public nint Data;
        public nint Hook;
        public nint Template;
    }

    [DllImport("comdlg32.dll", EntryPoint = "ChooseColorW")]
    static extern bool ChooseColor(ref ChooseColorData data);

    [DllImport("user32.dll")]
    static extern nint GetDC(nint window);

    [DllImport("user32.dll")]
    static extern int ReleaseDC(nint window, nint dc);

    [DllImport("gdi32.dll")]
    static extern uint GetPixel(nint dc, int x, int y);

    // Start at the color given, open the custom side, and take any color.
    const uint RgbInit = 0x1, FullOpen = 0x2, AnyColor = 0x100;
    const uint Invalid = 0xFFFFFFFF;
    static readonly uint[] custom = Enumerable.Repeat(0xFFFFFFu, 16).ToArray();

    /// <summary>
    /// Opens the dialog over <paramref name="owner"/>, starting at the color of
    /// <paramref name="argb"/>; the color picked, opaque, or null when none was.
    /// </summary>
    internal static uint? Choose(nint owner, uint argb)
    {
        var pinned = GCHandle.Alloc(custom, GCHandleType.Pinned);
        try
        {
            var data = new ChooseColorData
            {
                Size = Marshal.SizeOf<ChooseColorData>(),
                Owner = owner,
                Result = ToColorRef(argb),
                CustomColors = pinned.AddrOfPinnedObject(),
                Flags = RgbInit | FullOpen | AnyColor,
            };
            return ChooseColor(ref data) ? FromColorRef(data.Result) : null;
        }
        finally
        {
            pinned.Free();
        }
    }

    /// <summary>The pixel at a point on the screen, in pixels, as an opaque color; null when it cannot be read.</summary>
    internal static uint? PixelAt(int x, int y)
    {
        var dc = GetDC(0);
        if (dc == 0)
        {
            return null;
        }
        try
        {
            var pixel = GetPixel(dc, x, y);
            return pixel == Invalid ? null : FromColorRef(pixel);
        }
        finally
        {
            ReleaseDC(0, dc);
        }
    }

    static uint ToColorRef(uint argb) => (argb >> 16 & 0xFF) | (argb & 0xFF00) | (argb & 0xFF) << 16;
    static uint FromColorRef(uint color) => 0xFF000000u | (color & 0xFF) << 16 | (color & 0xFF00) | (color >> 16 & 0xFF);
}
