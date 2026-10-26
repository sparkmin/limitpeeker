using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Windows.Controls;
using System.Windows.Markup;

namespace LimitPeeker.Settings;

/// <summary>
/// The settings window's and the editor's words, in the language chosen: one
/// file of words for each language inside the program, English the one the
/// others follow and the one a missing word falls back to. Only these two
/// windows take it; the box, the popup and the notifications stay in English.
/// </summary>
internal static class Loc
{
    /// <summary>The languages there are words for, English first, then by code.</summary>
    internal static readonly string[] Languages = ["en", "es", "fr", "ja", "ko", "pt"];
    /// <summary>
    /// The windows' fonts for a language whose characters Segoe UI lacks and
    /// Malgun Gothic would draw in their Korean forms, as Japanese kanji.
    /// </summary>
    static readonly Dictionary<string, string> Fonts = new() { ["ja"] = "Segoe UI, Yu Gothic UI, Malgun Gothic" };
    /// <summary>
    /// The languages whose lines break only at spaces, as Korean is set. WPF
    /// breaks Hangul between any two syllables, so a word joiner goes between
    /// the letters of each word.
    /// </summary>
    static readonly string[] BrokenAtSpaces = ["ko"];
    const char Joiner = '\u2060';
    static readonly Dictionary<string, string> English = Load("en") ?? [];
    static Dictionary<string, string> words = English;

    /// <summary>The language the words are in now.</summary>
    internal static string Language { get; private set; } = "en";

    /// <summary>The fonts the windows take for the language, or none to keep their own.</summary>
    internal static string? Font => Fonts.GetValueOrDefault(Language);

    /// <summary>
    /// Takes the language `code` names, or with none Windows' display
    /// language when there are words for it, else English.
    /// </summary>
    internal static void Use(string? code)
    {
        var wanted = string.IsNullOrEmpty(code) ? CultureInfo.CurrentUICulture.TwoLetterISOLanguageName : code;
        Language = Languages.Contains(wanted) ? wanted : "en";
        words = Language == "en" ? English : Load(Language) ?? English;
    }

    /// <summary>A language's name in itself, as its file gives it, to pick it by.</summary>
    internal static string NameOf(string code) =>
        (Load(code) is { } words && words.TryGetValue("language.name", out var name) ? name : code)
            .Replace(Joiner.ToString(), "");

    /// <summary>The language a command line names with --language=, if any.</summary>
    internal static string? From(IEnumerable<string> args) =>
        args.FirstOrDefault(a => a.StartsWith("--language=", StringComparison.Ordinal))?["--language=".Length..];

    /// <summary>The words for `key`, in English when the language has none.</summary>
    internal static string T(string key) =>
        words.TryGetValue(key, out var text) || English.TryGetValue(key, out text) ? text : key;

    /// <summary>The words for `key`, each {name} in them replaced by its value.</summary>
    internal static string T(string key, params (string Name, object Value)[] values)
    {
        var text = T(key);
        foreach (var (name, value) in values)
        {
            text = text.Replace("{" + name + "}", Convert.ToString(value, CultureInfo.CurrentCulture));
        }
        return text;
    }

    /// <summary>
    /// The words for `key` without word joiners, which only a text block
    /// that may wrap needs: for a list's items, tool tips, buttons, names for
    /// screen readers, and what is kept, as a preset's name.
    /// </summary>
    internal static string Plain(string key, params (string Name, object Value)[] values) =>
        T(key, values).Replace(Joiner.ToString(), "");

    static Dictionary<string, string>? Load(string code)
    {
        using var stream = typeof(Loc).Assembly.GetManifestResourceStream($"LimitPeeker.Settings.Strings.{code}.json");
        var words = stream is null ? null : JsonSerializer.Deserialize<Dictionary<string, string>>(stream);
        return words is not null && BrokenAtSpaces.Contains(code)
            ? words.ToDictionary(word => word.Key, word => Joined(word.Value))
            : words;
    }

    /// <summary>A word joiner between two letters of a word where either is Hangul.</summary>
    static string Joined(string text)
    {
        static bool Hangul(char c) => c is (>= '\uAC00' and <= '\uD7A3') or (>= '\u1100' and <= '\u11FF') or (>= '\u3130' and <= '\u318F');
        var joined = new StringBuilder(text.Length * 2);
        for (var at = 0; at < text.Length; at++)
        {
            if (at > 0 && !char.IsWhiteSpace(text[at - 1]) && !char.IsWhiteSpace(text[at])
                && (Hangul(text[at - 1]) || Hangul(text[at])))
            {
                joined.Append(Joiner);
            }
            joined.Append(text[at]);
        }
        return joined.ToString();
    }
}

/// <summary>
/// The words for a key in XAML, as Text="{l:T boxes.place.title}": with word
/// joiners for a text block, which may wrap, and plain for the rest, such as
/// a window's title or a name for screen readers.
/// </summary>
[MarkupExtensionReturnType(typeof(string))]
public sealed class TExtension(string key) : MarkupExtension
{
    [ConstructorArgument("key")]
    public string Key { get; set; } = key;

    public override object ProvideValue(IServiceProvider serviceProvider) =>
        serviceProvider.GetService(typeof(IProvideValueTarget)) is IProvideValueTarget { TargetProperty: var property }
            && property == TextBlock.TextProperty
            ? Loc.T(Key)
            : Loc.Plain(Key);
}
