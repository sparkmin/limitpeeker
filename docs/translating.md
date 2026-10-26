# Translating the settings window

The settings window and the box editor take their words from one file for each language in `settings/Strings`.
`en.json` is the source; every other file has the same keys in the same order. Pick the language under `Language` on the
settings window's `General` page. The boxes, the popup, the notifications and the log stay in English.

## The files

- Each is one flat JSON object of `"key": "words"`, in UTF-8.
- A key names a place on the screen: `boxes.place.title` and `boxes.place.caption` for the title and the line under it
  on the `Boxes` page, `general.refresh.button`, `editor.toolbar.align`, `message.delete_preset`.
- `language.name` comes first: the language's name in itself, as the `Language` list shows it (`English`, `한국어`).
- A sentence is one key, never put together from pieces. Values go in named places, as in `{count} blocks picked`.
  Keep every place with its name, and put it where the language needs it.
- The files are built into `LimitPeeker.Settings.exe`, so nothing is added next to it.

## Words kept as they are

- LimitPeeker, and Windows
- The services: Codex, Claude, Antigravity, Grok, PC
- The labels of limits and meters: 5h, 7d, Fable 7d, CPU, RAM, GPU, VRAM
- The names people give boxes and presets, the sample presets' names, and the looks: Slim, Inline, Stack, Pie, Rings
- Files, options, keys and units: uninstall.exe, limitpeeker.exe --remove, AppData, Ctrl, Shift, Delete, DIP, %, #RRGGBB

Everything else is translated, `Used`, `Remaining` and `Show values as` too, so the settings window may word them
differently from the popup.

## Style

Follow the Windows 11 Settings app in that language. In Korean, descriptions end in -합니다, what to do in -하세요, and
titles and buttons are short.

## Adding a language

1. Copy `en.json` to `<code>.json`, with the two-letter code Windows gives the language (`ja`, `de`), and translate the
   values. Keep the keys and their order.
2. Add the code to `Languages` in `settings/Loc.cs` and to `LANGUAGES` in `src/settings_link.rs`, and the code, the
   language's name in itself and the file to `FILES` in the tests in `src/settings_words.rs`.
3. If the language breaks lines only at spaces, as Korean does, add it to `BrokenAtSpaces` in `settings/Loc.cs`. The
   window then keeps each word whole; otherwise a line may break inside a word.
4. If Segoe UI lacks the language's script, add the language's fonts to `Fonts` in `settings/Loc.cs`, as Japanese has
   `Yu Gothic UI` there; Chinese would take `Microsoft YaHei UI`. Otherwise the windows draw such characters in Malgun
   Gothic, in their Korean forms.
5. Run `cargo test`. It checks that the files have the same keys, that no value is empty, that each value keeps its
   places, that the names of the product, the services, files and keys stay, that every key is used, and that no words
   are written into the windows instead.
6. Build, pick the language, and look at every page and at the box editor for words cut off or running over.

`Windows default` in the `Language` list picks the language of Windows' display when there is a file for it, and
English otherwise.

## Changing the English

When the meaning of a key's English changes, rename the key. The other files then miss it, the tests fail, and it is
translated again. A fix that keeps the meaning, such as a typo, keeps the key.

## Translating with a model

Give it `en.json`, the words kept above, and the language's file as it is. Ask for a JSON object of the keys that are
new or changed, and nothing else. Then run the tests and look at the window.
