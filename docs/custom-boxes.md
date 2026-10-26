# Custom boxes

After the five looks, the popup's Style line and the settings window's Style list hold your own custom boxes, and a
sample while a box uses it; the other samples are the editor's, to start from. A custom box is a box made by stacking
blocks in columns (`columns`) or by placing them freely (`free`); `Columns sample` and `Free sample` come as examples in
the editor. When one is chosen, its blocks decide the services and limits, so Background, Services and Limits are
greyed out; `Show values as` (used, remaining) still applies.

Each of the five presets also comes as a sample made of blocks, to start a box of your own from:

- `AI slim`, `AI inline`, `AI stack`, `AI pie` and `AI rings` show Codex, Claude and Antigravity, each with its
  first two limits
  (by place, so they fit whatever the plan names them).
- `PC slim`, `PC inline`, `PC stack`, `PC pie` and `PC rings` show PC's CPU, RAM, GPU and VRAM.

They look close to the presets but not the same: a custom box is one box, with no card for each service, and a dial's
value has a `%`. In the `columns` ones (slim, pie, rings), a limit a service does not have is left out and the box closes
up; in the `free` ones (inline, stack), its place stays empty. Choose one and press `Duplicate` to change it.

## The editor

Boxes are made and changed in the editor window. Open it with `Open editor` on the settings window's `Custom boxes` page
(or beside `Services` on its `Boxes` page, when the box is a custom one), or with `Customize…` at the end of the popup's
Style line.

- **Presets:** the left of the window holds `My presets`, then the samples on three shelves: `Basic samples`,
  `AI box samples` and `PC box samples`. Click a shelf's head to fold or unfold it; the AI and PC shelves start folded,
  and LimitPeeker keeps which are folded. The samples themselves cannot be changed.
- **New:** press `Create new preset` (Ctrl+N) and choose a name, a style (`Columns style`, `Free style`) and under
  `Start from` a sample or one of your presets of that style to start from; at first it is that style's own sample. The
  style cannot be changed afterwards. `Duplicate` under the list copies the chosen preset.
- **Column widths:** drag the line between the presets on the left and the blocks on the right to change their widths.
- **Changing:** a preset opens with no block chosen and its block list at the top; pick a block in the list or on the
  stage. `Add ▾`, delete, up and down above the block list handle the blocks, and below them you change the chosen
  block's properties. Colors are picked from the palette of the swatch button, typed as #RRGGBB, or taken from anywhere on the screen with the eyedropper beside the field. The picture on the stage
  follows, laid out as on the taskbar.
- **Order:** in a free style, the top of the block list is drawn in front. Up and down move the chosen block or group one
  step forward or back. In a columns style, the list starts with the left column.
- **Save and use:** `Save` (Ctrl+S) saves only the file. To use it on the taskbar, press `Use on taskbar`. A preset in use
  shows `In use`, and saving it changes the taskbar too.

## The stage

The stage marks the origin X and Y are measured from (`0, 0`), and the top-left point the chosen block's X and Y refer to
(a blue dot). In a columns style, dashed lines and column numbers show between the columns; turn on `Show sizes` at the
top right of the stage to see the box's and the chosen block's sizes as on a drawing. The box next to it chooses a zoom
(2×, 3×, 4×). On the stage, Ctrl and the mouse wheel zoom from 1× to 8× around the pointer. Dragging with the wheel button,
or with Space and the left button, moves the view in any direction; the wheel moves it up and down, and Shift with the
wheel left and right. The box never leaves the screen.

A free style is handled like a slide.

- **Choosing:** press and drag to move; with Shift held, it moves only across or only up and down. Click with Ctrl or
  Shift held, or drag over an empty spot, to choose several.
- **Size:** with one block or one group chosen, the white handles change its size; with Shift held, the ratio stays. A
  group's blocks grow and shrink together, text sizes too.
- **A block in a group:** with a group chosen, click one of its blocks once more to choose only that block. Click outside
  the group to come out.
- **Arranging:** Align, Distribute and Group on the toolbar line blocks up, space them and group them. Group is Ctrl+G and
  ungroup Ctrl+Shift+G.
- **Inside the box:** blocks do not leave the box on the left, top or bottom. Past the right end, the box widens (up to
  600 DIP). The box is always 40 DIP high. `Fit to blocks` fits the width to the blocks. `Background` on the same line
  sets the taskbar box's color and opacity, as the `Boxes` page does, and the stage shows the preset on it.
- **Keys and values:** the arrow keys move by 1 DIP, with Shift by 5 DIP; Ctrl+Z and Ctrl+Y undo and redo. The X, Y, W
  and H fields are for fine changes.

## The file

Each box is saved as one JSON file in `%LOCALAPPDATA%\LimitPeeker\boxes`. You may change the files yourself; the change
shows the next time the popup opens. A file with a mistake is left out, and the log says why. The format is:

```json
{
  "name": "Free sample",
  "mode": "free",
  "width": 112,
  "blocks": [
    { "block": "ring", "service": "claude", "limit": 1, "x": 0, "y": 4, "w": 32, "h": 32 },
    { "block": "text", "service": "claude", "limit": 1, "size": 9, "align": "center",
      "x": 0, "y": 13.5, "w": 32, "h": 13 },
    { "block": "icon", "service": "gpt", "x": 40, "y": 5 },
    { "block": "text", "service": "gpt", "limit": 1, "show": "both", "x": 60, "y": 6, "w": 52, "h": 14 },
    { "block": "bar", "service": "gpt", "limit": 1, "x": 40, "y": 28, "w": 72, "h": 4 }
  ]
}
```

- `mode`: with `columns`, put the list of columns in `columns`. Each column holds 1–2 blocks, stacked from the top. With
  `free`, give `width` (DIP) and `blocks`, and for each block `x`, `y` and, where needed, `w` and `h` (DIP). The box is 40
  DIP high.
- `block`: `icon`, `ring`, `rings` (concentric rings, 1–3 in `limits`), `pie`, `bar` (`width`, `thickness`), `text`,
  `space` (`width`).
- `service`: one of the services LimitPeeker found: `gpt` (Codex), `claude`, `gemini` (Antigravity), `grok`, or `pc`
  (whose limits are `CPU`, `RAM`, `GPU` and `VRAM`). Up to four in a box.
- `limit`: its place counted from 1, or its name (`"7d"`). Without it, 1.
- `text`: `show` shows one of `value`, `label`, `both`, `resets` and `name`, or `"text": "words"` shows those words as
  they are. `resets` is how long until the limit resets, as the popup says it (`4h 51m`, `2d 3h`), in the quiet color
  of a label; `--` when that is not known. It is as wide as its longest form, so the box does not move as it counts
  down.
  `size` (8–24), `weight` (100–900) and `align` (`start`, `center`, `end`) may be given.
- `color`: `#RRGGBB`. When the value is low, orange or red takes over, unless the warning colors are off.
