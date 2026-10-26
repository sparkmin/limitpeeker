namespace LimitPeeker.Settings;

/// <summary>One of a service's limits, and whether it shows.</summary>
internal sealed record Limit(string Label, bool Shown);

/// <summary>A service the controller offers, whether it shows, and its limits.</summary>
internal sealed record Service(string Id, string Name, bool Shown, List<Limit> Limits);

/// <summary>
/// A custom box: its name, its style ("columns" or "free"), whether it is a sample, and its shelf in the editor
/// ("mine", "basic", "ai" or "pc").
/// </summary>
internal sealed record BoxInfo(string Name, string Style, bool Sample, string Shelf);

/// <summary>
/// A Task Scheduler task that starts a limitpeeker.exe at sign-in: where it is, whether it is on, whether it starts
/// this one, what keeps the box away ("admin", "hidden" or none), and the program it starts.
/// </summary>
internal sealed record LogonTask(string Path, bool Enabled, bool ThisExe, string Problem, string Program);

/// <summary>
/// The whole state as the controller sent it, lines of tab-split words: the
/// look and the looks to pick from, the fill mode, the background, how many
/// services and limits may show, when Refresh now is taken again, each custom
/// box, and each service with its limits after it. Must match
/// src/settings_link.rs.
/// </summary>
internal sealed class State
{
    public string Look { get; private set; } = "";
    public List<string> Looks { get; } = [];
    public string Fill { get; private set; } = "";
    public string Background { get; private set; } = "";
    /// <summary>The chosen box's color as #AARRGGBB, or "default" for the one every look starts with.</summary>
    public string Color { get; private set; } = "default";
    public int MostServices { get; private set; } = 1;
    public int MostLimits { get; private set; } = 1;
    /// <summary>Services that show more limits than MostLimits: PC, four.</summary>
    readonly Dictionary<string, int> mostOf = [];
    public int MostLimitsOf(string id) => mostOf.TryGetValue(id, out var most) ? most : MostLimits;
    /// <summary>
    /// When, in Unix milliseconds, the controller takes Refresh now again,
    /// wherever it was pressed: 0 before the first, and null with the mock
    /// figures, which never take it.
    /// </summary>
    public long? RefreshAt { get; private set; } = 0;
    public List<Service> Services { get; } = [];
    public List<BoxInfo> Boxes { get; } = [];
    /// <summary>The services the popup's summary shows, in its order, apart from the box's.</summary>
    public List<string> Summary { get; } = [];
    /// <summary>
    /// Whether LimitPeeker starts at sign-in: "on", "off", or "blocked" when
    /// Windows' Startup apps turned it off.
    /// </summary>
    public string Startup { get; private set; } = "off";
    /// <summary>The Task Scheduler tasks a person made that start it at sign-in.</summary>
    public List<LogonTask> Tasks { get; } = [];
    /// <summary>
    /// The boxes by what they show, the one chosen, where it goes, whether it
    /// can move left or right among the boxes there, and whether a box may
    /// be dragged.
    /// </summary>
    public List<string> Tiles { get; } = [];
    /// <summary>Whether each box, in the same order, is hidden, kept but not on the taskbar.</summary>
    public List<bool> HiddenTiles { get; } = [];
    /// <summary>
    /// The controller's version, and the public repository as owner/name
    /// whose newest release Check for updates asks for, empty until there is
    /// one.
    /// </summary>
    public string Version { get; private set; } = "";
    public string Repository { get; private set; } = "";
    public int Current { get; private set; }
    public string Place { get; private set; } = "tray";
    public bool CanMoveEarlier { get; private set; }
    public bool CanMoveLater { get; private set; }
    public bool Drag { get; private set; } = true;
    /// <summary>Whether each tool's own update command runs; off at first.</summary>
    public bool Updates { get; private set; }
    /// <summary>Whether low values warn in color.</summary>
    public bool Warn { get; private set; } = true;
    /// <summary>
    /// Which notifications are on, as near, full, back and failing, and the
    /// share used that is almost used up.
    /// </summary>
    public Dictionary<string, bool> Notify { get; } = [];
    public string NotifyAt { get; private set; } = "90";
    /// <summary>The language picked for this window, empty for Windows'.</summary>
    public string Language { get; private set; } = "";
    /// <summary>The editor's shelves folded, by their words.</summary>
    public List<string> Folded { get; } = ["ai", "pc"];

    public static State Parse(string text)
    {
        var state = new State();
        foreach (var line in text.Split('\n'))
        {
            switch (line.Split('\t'))
            {
                case ["look", var look]:
                    state.Look = look;
                    break;
                case ["looks", .. var looks]:
                    state.Looks.AddRange(looks);
                    break;
                case ["fill", var fill]:
                    state.Fill = fill;
                    break;
                case ["background", var background]:
                    state.Background = background;
                    break;
                case ["color", var color]:
                    state.Color = color;
                    break;
                case ["most", var services, var limits, .. var others]:
                    state.MostServices = int.TryParse(services, out var most) ? most : 1;
                    state.MostLimits = int.TryParse(limits, out most) ? most : 1;
                    for (var at = 0; at + 1 < others.Length; at += 2)
                    {
                        if (int.TryParse(others[at + 1], out most))
                        {
                            state.mostOf[others[at]] = most;
                        }
                    }
                    break;
                case ["refresh", var at]:
                    state.RefreshAt = long.TryParse(at, out var ms) ? ms : null;
                    break;
                case ["box", var name, var style, var sample, var shelf]:
                    state.Boxes.Add(new BoxInfo(name, style, sample == "1", shelf));
                    break;
                case ["folded", .. var folded]:
                    state.Folded.Clear();
                    state.Folded.AddRange(folded);
                    break;
                case ["service", var id, var name, var shown]:
                    state.Services.Add(new Service(id, name, shown == "1", []));
                    break;
                case ["limit", var label, var shown] when state.Services.Count > 0:
                    state.Services[^1].Limits.Add(new Limit(label, shown == "1"));
                    break;
                case ["summary", .. var summary]:
                    state.Summary.AddRange(summary);
                    break;
                case ["startup", var startup]:
                    state.Startup = startup;
                    break;
                case ["task", var path, var enabled, var thisExe, var problem, var program]:
                    state.Tasks.Add(new LogonTask(path, enabled == "1", thisExe == "1", problem, program));
                    break;
                case ["boxes", var current, .. var names]:
                    state.Current = int.TryParse(current, out var chosen) ? chosen : 0;
                    state.Tiles.AddRange(names);
                    break;
                case ["hidden", .. var hidden]:
                    state.HiddenTiles.AddRange(hidden.Select(mark => mark == "1"));
                    break;
                case ["place", var place]:
                    state.Place = place;
                    break;
                case ["order", var earlier, var later]:
                    state.CanMoveEarlier = earlier == "1";
                    state.CanMoveLater = later == "1";
                    break;
                case ["drag", var drag]:
                    state.Drag = drag == "on";
                    break;
                case ["updates", var updates]:
                    state.Updates = updates == "on";
                    break;
                case ["warn", var warn]:
                    state.Warn = warn == "on";
                    break;
                case ["notify", var near, var full, var back, var failing, var at]:
                    state.Notify["near"] = near == "1";
                    state.Notify["full"] = full == "1";
                    state.Notify["back"] = back == "1";
                    state.Notify["failing"] = failing == "1";
                    state.NotifyAt = at;
                    break;
                case ["language", var language]:
                    state.Language = language;
                    break;
                case ["version", var version]:
                    state.Version = version;
                    break;
                case ["repository", var repository]:
                    state.Repository = repository;
                    break;
            }
        }
        return state;
    }
}
