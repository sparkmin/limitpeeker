using System.Globalization;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace LimitPeeker.Settings;

/// <summary>
/// A limit a block names: its place, from 1, or its label. By its place a box
/// fits plans whose limits are named otherwise.
/// </summary>
internal sealed record Which(int Nth, string? Label)
{
    internal static readonly Which First = new(1, null);

    /// <summary>The key a list of limits uses: "#2" for the second, "=7d" for one so named.</summary>
    internal string Key => Label is { } label ? "=" + label : "#" + Nth.ToString(CultureInfo.InvariantCulture);

    internal static Which? FromKey(string? key) => key switch
    {
        ['=', .. var label] when label.Length > 0 => new Which(0, label),
        ['#', .. var nth] when int.TryParse(nth, CultureInfo.InvariantCulture, out var n) && n >= 1 => new Which(n, null),
        _ => null,
    };

    internal JsonNode ToJson() => Label is { } label ? JsonValue.Create(label)! : JsonValue.Create(Nth);

    internal static Which? From(JsonNode? node)
    {
        if (node is not JsonValue value)
        {
            return null;
        }
        if (value.TryGetValue<string>(out var label))
        {
            return new Which(0, label);
        }
        return value.TryGetValue<double>(out var n) && n >= 1 ? new Which((int)n, null) : null;
    }
}

/// <summary>
/// One block as the editor holds it, with every field a box file may give it.
/// In columns a block may sit under the one before; in a free box it has a
/// place. Sizes are in DIPs; null is the controller's default.
/// </summary>
internal sealed class BlockModel
{
    internal string Kind = "ring";
    internal string? Service;
    internal List<Which> Limits = [Which.First];
    internal string Show = "value";
    internal string? Own;
    internal double? Size;
    internal double? Weight;
    internal string Align = "start";
    internal double? Width;
    internal double? Thickness;
    internal string? Color;
    internal bool Stacked;
    internal double X;
    internal double Y;
    internal double? W;
    internal double? H;
    /// <summary>In a free box, the group it moves with; the controller does not read it.</summary>
    internal int? Group;

    /// <summary>Whether it shows one of its service's limits, not a name or words of its own.</summary>
    internal bool ReadsLimit => Kind switch
    {
        "ring" or "rings" or "pie" or "bar" => true,
        "text" => Own is null && Show != "name",
        _ => false,
    };

    /// <summary>Whether it names a service.</summary>
    internal bool NamesService => Kind != "space" && !(Kind == "text" && Own is not null);

    internal BlockModel Copy()
    {
        var copy = (BlockModel)MemberwiseClone();
        copy.Limits = [.. Limits];
        return copy;
    }

    internal static BlockModel From(JsonObject json)
    {
        var block = new BlockModel
        {
            Kind = Text(json, "block") ?? "ring",
            Service = Text(json, "service"),
            Show = Text(json, "show") ?? "value",
            Own = Text(json, "text"),
            Size = Number(json, "size"),
            Weight = Number(json, "weight"),
            Align = Text(json, "align") ?? "start",
            Width = Number(json, "width"),
            Thickness = Number(json, "thickness"),
            Color = Text(json, "color"),
            X = Number(json, "x") ?? 0,
            Y = Number(json, "y") ?? 0,
            W = Number(json, "w"),
            H = Number(json, "h"),
            Group = Number(json, "group") is { } group ? (int)group : null,
        };
        if (json["limits"] is JsonArray limits)
        {
            block.Limits = limits.Select(Which.From).OfType<Which>().ToList();
        }
        else if (Which.From(json["limit"]) is { } limit)
        {
            block.Limits = [limit];
        }
        if (block.Limits.Count == 0)
        {
            block.Limits = [Which.First];
        }
        return block;
    }

    /// <summary>The block as a box file writes it, with only the fields its kind takes.</summary>
    internal JsonObject ToJson(bool free)
    {
        var json = new JsonObject { ["block"] = Kind };
        if (NamesService && Service is { } service)
        {
            json["service"] = service;
        }
        if (Kind == "rings")
        {
            json["limits"] = new JsonArray(Limits.Take(3).Select(l => (JsonNode?)l.ToJson()).ToArray());
        }
        else if (ReadsLimit)
        {
            json["limit"] = (Limits.FirstOrDefault() ?? Which.First).ToJson();
        }
        if (Kind == "text")
        {
            if (Own is not null)
            {
                json["text"] = Own;
            }
            else
            {
                json["show"] = Show;
            }
            Put(json, "size", Size);
            Put(json, "weight", Weight);
            if (Align != "start")
            {
                json["align"] = Align;
            }
        }
        if (!free && Kind is "bar" or "space")
        {
            Put(json, "width", Width);
        }
        if (Kind is "bar" or "ring" or "rings")
        {
            Put(json, "thickness", Thickness);
        }
        if (Kind is not ("space" or "icon") && !string.IsNullOrWhiteSpace(Color))
        {
            json["color"] = Color.Trim();
        }
        if (free)
        {
            json["x"] = Round(X);
            json["y"] = Round(Y);
            Put(json, "w", W);
            Put(json, "h", H);
            if (Group is { } group)
            {
                json["group"] = group;
            }
        }
        return json;
    }

    static void Put(JsonObject json, string key, double? value)
    {
        if (value is { } v)
        {
            json[key] = Round(v);
        }
    }

    internal static double Round(double value) => Math.Round(value, 2);

    static string? Text(JsonObject json, string key) =>
        json[key] is JsonValue value && value.TryGetValue<string>(out var text) ? text : null;

    static double? Number(JsonObject json, string key) =>
        json[key] is JsonValue value && value.TryGetValue<double>(out var number) ? number : null;
}

/// <summary>
/// A custom box as the editor holds it: its name, whether its blocks are in
/// columns or placed freely, its width when free, and its blocks as the editor
/// lists them, the columns from the left, each top to bottom. The controller
/// checks it; this only reads and writes the file's form.
/// </summary>
internal sealed class BoxModel
{
    internal string Name = "";
    internal bool Free;
    internal double Width = 120;
    internal List<BlockModel> Blocks = [];

    static readonly JsonSerializerOptions Written = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    internal static BoxModel Parse(string text)
    {
        var json = JsonNode.Parse(text) as JsonObject ?? throw new FormatException("not a box");
        var box = new BoxModel
        {
            Name = json["name"] is JsonValue name && name.TryGetValue<string>(out var n) ? n : "",
            Free = json["mode"] is JsonValue mode && mode.TryGetValue<string>(out var m) && m == "free",
        };
        if (json["width"] is JsonValue width && width.TryGetValue<double>(out var w))
        {
            box.Width = w;
        }
        if (box.Free)
        {
            foreach (var block in json["blocks"] as JsonArray ?? new JsonArray())
            {
                if (block is JsonObject o)
                {
                    box.Blocks.Add(BlockModel.From(o));
                }
            }
        }
        else
        {
            foreach (var column in json["columns"] as JsonArray ?? new JsonArray())
            {
                var first = true;
                foreach (var block in column as JsonArray ?? new JsonArray())
                {
                    if (block is JsonObject o)
                    {
                        var model = BlockModel.From(o);
                        model.Stacked = !first;
                        first = false;
                        box.Blocks.Add(model);
                    }
                }
            }
        }
        return box;
    }

    /// <summary>
    /// The box as its file is written. In columns a block under the one
    /// before joins its column when that has one block; otherwise it starts
    /// a column of its own.
    /// </summary>
    internal string ToJson()
    {
        var json = new JsonObject { ["name"] = Name, ["mode"] = Free ? "free" : "columns" };
        if (Free)
        {
            json["width"] = BlockModel.Round(Width);
            json["blocks"] = new JsonArray(Blocks.Select(b => (JsonNode?)b.ToJson(true)).ToArray());
        }
        else
        {
            var columns = new JsonArray();
            JsonArray? column = null;
            foreach (var block in Blocks)
            {
                if (block.Stacked && column is { Count: 1 })
                {
                    column.Add(block.ToJson(false));
                }
                else
                {
                    column = new JsonArray(block.ToJson(false));
                    columns.Add(column);
                }
            }
            json["columns"] = columns;
        }
        return json.ToJsonString(Written);
    }

    internal BoxModel Copy() => Parse(ToJson());
}
