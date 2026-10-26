using System.Runtime.InteropServices;

namespace LimitPeeker.Settings;

/// <summary>
/// The link to the controller's tray window. This window sends one command a
/// message and takes the whole state whenever it changes, both lines of
/// tab-split words as UTF-16 in WM_COPYDATA. Must match src/settings_link.rs.
/// </summary>
internal static class Link
{
    internal const nuint Command = 0x4C500001;
    internal const nuint State = 0x4C500002;
    internal const nuint Show = 0x4C500003;
    /// <summary>For the editor: a box to open, a draft's preview, and what a save or delete came to.</summary>
    internal const nuint Box = 0x4C500004;
    internal const nuint Preview = 0x4C500005;
    internal const nuint Result = 0x4C500006;
    /// <summary>A box's file is up to 64 KB.</summary>
    internal const int Longest = 131072;
    internal const string Hello = "hello";
    /// <summary>Every service read at once, as from the popup.</summary>
    internal const string Refresh = "refresh";
    /// <summary>The window come to the front: Task Scheduler's sign-in tasks are looked for anew.</summary>
    internal const string StartupCheck = "startup-check";
    internal const int WmCopyData = 0x004A;
    const uint AbortIfHung = 0x0002;
    const string ControllerArgument = "--controller=";

    [StructLayout(LayoutKind.Sequential)]
    struct CopyData
    {
        public nuint Kind;
        public uint Bytes;
        public nint Data;
    }

    [DllImport("user32.dll", EntryPoint = "SendMessageTimeoutW")]
    static extern nint SendMessageTimeout(nint window, uint message, nint from, ref CopyData data,
        uint flags, uint timeout, out nuint result);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool IsWindow(nint window);

    /// <summary>The controller's window, from its argument, or zero.</summary>
    internal static nint ControllerFrom(string[] args)
    {
        foreach (var arg in args)
        {
            if (arg.StartsWith(ControllerArgument, StringComparison.Ordinal)
                && nuint.TryParse(arg[ControllerArgument.Length..], out var handle))
            {
                return (nint)handle;
            }
        }
        return 0;
    }

    /// <summary>
    /// Sends the controller one command, giving up after a second rather than
    /// hang on a controller that does not answer.
    /// </summary>
    internal static bool Send(nint controller, nint from, string command)
    {
        var data = Marshal.StringToHGlobalUni(command);
        try
        {
            var copy = new CopyData { Kind = Command, Bytes = (uint)command.Length * 2, Data = data };
            return SendMessageTimeout(controller, WmCopyData, from, ref copy, AbortIfHung, 1000, out _) != 0;
        }
        finally
        {
            Marshal.FreeHGlobal(data);
        }
    }

    /// <summary>The text of a WM_COPYDATA of this kind, or null.</summary>
    internal static string? Read(nint message, nuint kind)
    {
        var copy = Marshal.PtrToStructure<CopyData>(message);
        if (copy.Kind != kind || copy.Bytes % 2 != 0 || copy.Bytes > Longest * 2)
        {
            return null;
        }
        return copy.Bytes == 0 || copy.Data == 0 ? "" : Marshal.PtrToStringUni(copy.Data, (int)copy.Bytes / 2);
    }
}
