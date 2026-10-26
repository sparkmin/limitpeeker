using System.Windows;

namespace LimitPeeker.Settings;

/// <summary>
/// The settings window, started by the controller with its tray window's
/// handle.
/// </summary>
public partial class App : Application
{
    /// <summary>
    /// The words' language, taken before App.xaml's own resources are read,
    /// which have words too.
    /// </summary>
    public App() => Loc.Use(Loc.From(Environment.GetCommandLineArgs()));

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        // With --editor, the controller wants the box editor alone: the
        // settings window only makes its handle, for the link.
        // A page to open on, as the window opened again in another language
        // asks.
        var page = e.Args.FirstOrDefault(a => a.StartsWith("--page=", StringComparison.Ordinal))?["--page=".Length..];
        var window = new MainWindow(Link.ControllerFrom(e.Args), e.Args.Contains("--editor"), page);
        if (window.EditorOnly)
        {
            new System.Windows.Interop.WindowInteropHelper(window).EnsureHandle();
        }
        else
        {
            window.Show();
        }
    }
}
