# Scene document format

A scene document is JSON. The visual-novel editor, the `miu-cli` tool, and the runtime
loader all read the same format, so a scene authored in the editor can be rendered by
CI without opening the editor.

```json
{
  "version": 1,
  "viewport": [960, 540],
  "background": "#0b0d14",
  "camera": { "position": [480, 270], "zoom": 1.0, "rotation": 0 },
  "assets": {
    "textures": { "backdrop": "art/backdrop.png" },
    "fonts": { "body": "fonts/Body.ttf" }
  },
  "nodes": [
    { "type": "sprite", "name": "backdrop", "texture": "backdrop",
      "size": [960, 540], "pivot": [0.5, 0.5], "position": [480, 270], "layer": 0 }
  ],
  "animations": [
    { "name": "idle", "duration": 3.0, "loop": true,
      "tracks": [ { "node": "backdrop", "property": "y",
                    "keys": [[0, 270, "ease_in_out"], [3, 264]] } ] }
  ]
}
```

## Top level

| Key | Type | Notes |
| --- | --- | --- |
| `version` | number | Document version; `1` is current. |
| `viewport` | `[w, h]` | Authored size in pixels. Required. |
| `background` | colour string | Clear colour; omitted means transparent. |
| `camera` | object | `position`, `zoom`, `rotation` (degrees). |
| `assets` | object | `textures` and `fonts`, each a name → path map. |
| `nodes` | array | Root-level nodes. |
| `animations` | array | Independent animations. |

Paths are resolved by the host, relative to whatever root it chooses. The engine never
touches the file system, which is what lets the same document load from a zip, a
bundle, or memory.

## Nodes

Every node accepts:

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `type` | string | — | `group`, `sprite`, `text`, or `shape`. |
| `name` | string | `type` + counter | Animations reference nodes by name. |
| `position` | `[x, y]` | `[0, 0]` | World units. |
| `rotation` | number | `0` | Degrees. |
| `scale` | number or `[x, y]` | `1` | Uniform or per-axis. |
| `layer` | number | `0` | Lower layers draw first. |
| `opacity` | number | `1` | Multiplied into the subtree. |
| `visible` | bool | `true` | Hides the node and its children. |
| `blend` | string | `source_over` | `source_over`, `add`, `multiply`, `screen`, `copy`. |
| `children` | array | `[]` | Child nodes. |

### `sprite`

| Key | Notes |
| --- | --- |
| `texture` | Asset name. Required. |
| `size` | Destination size; zero uses the texture's size. |
| `pivot` | Normalised origin inside the destination, `[0.5, 1.0]` is bottom-centre. |
| `color` | Tint, straight alpha. |
| `source` | Atlas rectangle `[x, y, w, h]` in texels; omitted means the whole texture. |
| `flip_x`, `flip_y` | Mirrors sampling. |

### `text`

| Key | Notes |
| --- | --- |
| `font` | Asset name. Required. |
| `text` | UTF-8 content; `\n` starts a new line. |
| `size` | Em size in pixels (default `16`). |
| `line_height` | Multiplier on the font's natural line height. |
| `letter_spacing` | Extra pixels between glyphs. |
| `align` | `left`, `center`, `right`. |
| `wrap` | `none`, `word`, `character`. |
| `max_width` | Wrap width in world units. |
| `pivot` | Normalised origin inside the laid-out block. |

### `shape`

| Key | Notes |
| --- | --- |
| `shape` | `rect`, `round_rect`, `ellipse`, `polygon`, or `polyline` (default `rect`). |
| `rect` | `[x, y, w, h]` for rectangles. |
| `radius` | Corner radius for `round_rect`. |
| `center`, `radii` | Ellipse geometry. |
| `points` | Polygon/polyline vertices. |
| `fill` | Paint. |
| `stroke` | Paint for the outline. |
| `stroke_width` | Outline width in world units. |
| `stroke_cap` | `butt`, `round`, `square`. |
| `stroke_join` | `miter`, `round`, `bevel`. |
| `dash` | Dash pattern, e.g. `[8, 4]`. |
| `pivot` | Origin inside the shape bounds. |

## Paints

* `"#rgb"`, `"#rgba"`, `"#rrggbb"`, `"#rrggbbaa"` — solid colour.
* `[r, g, b]` or `[r, g, b, a]` — solid colour, components in `[0, 1]`.
* `{ "linear": { "from": [...], "to": [...], "stops": [[0, "#000"], [1, "#fff"]],
   "spread": "pad" } }`
* `{ "radial": { "center": [...], "radius": 128, "stops": [...] } }`
* `null` — no paint.

Stop colours use the same string/array forms as paints. `spread` accepts `pad`,
`repeat`, and `reflect`.

## Animations

| Key | Notes |
| --- | --- |
| `name` | Used by tools and scripting. |
| `duration` | Seconds. |
| `loop` | Whether the animation restarts at `duration`. |
| `tracks` | Array of tracks. |

A track targets one node property: `x`, `y`, `rotation`, `scale_x`, `scale_y`,
`opacity`, or `layer`. Each keyframe is `[time, value]` with an optional easing name:
`linear` (default), `ease_in`, `ease_out`, `ease_in_out`, or `step`.

Animations are absolute: applying time `t` twice writes the same values, so a host can
seek freely. Each animation advances on its own clock, either looping or clamping at
its end.

## Loading

Rust:

```rust
let text = std::fs::read_to_string("samples/scenes/visual_novel.miu.json")?;
let document = miu_scene::Document::parse(&text)?;
let files = read_assets(&document);         // path -> encoded bytes
let player = miu_scene::Player::from_document(&document, &files)?;
let png = player.render_at_size(1280, 720)?;
```

C ABI: parse, supply each declared asset, then instantiate. `miu_document_asset_path`
and `miu_document_asset_kind` enumerate what has to be supplied.

## Errors

`Document::parse` reports `Json`, `Unsupported`, `InvalidField`, `UnknownNode`,
`Scene`, and `Camera` failures. A missing asset is reported at instantiation, not at
parse time, because the host decides where assets come from.
