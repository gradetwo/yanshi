# 每个工具的可复制调用示例

**这份文件是生成出来的**（`scripts/tool-examples-doc.mjs`）—— 内容取自运行中服务端的 `GET /api/tools`，
所以它不会与实现漂移；`--check` 模式会在文档过期时失败。

当前共 52 个工具带示例，**全部经过实调验证**（见 `scripts/tool-example-acceptance.mjs`）。

## `begin_changeset`

```json
{}
```

## `blob_gc`

```json
{}
```

## `brush_preview`

```json
{
  "brush": "spray",
  "color": {
    "a": 255,
    "b": 255,
    "g": 64,
    "r": 0
  },
  "hardness": 0.6,
  "opacity": 0.8,
  "size": 24
}
```

## `brush_stroke`

```json
{
  "brush": "100%_Opaque",
  "color": {
    "a": 255,
    "b": 0,
    "g": 0,
    "r": 255
  },
  "layer_id": "L1",
  "points": [
    [
      100,
      100,
      1
    ],
    [
      180,
      140,
      1
    ],
    [
      260,
      100,
      1
    ]
  ],
  "size": 40
}
```

## `checkpoint`

```json
{}
```

## `collect_garbage`

```json
{}
```

## `comment`

```json
{
  "text": "x"
}
```

## `create_group`

```json
{
  "group_id": "x",
  "layer_id": "L1"
}
```

## `create_layer`

```json
{
  "layer_id": "L1",
  "name": "Layer 1"
}
```

## `delete_layer`

```json
{
  "layer_id": "L1"
}
```

## `delete_object`

```json
{
  "object_id": "o1"
}
```

## `draw_shape`

```json
{
  "data": {
    "geometry": {
      "bbox": {
        "h": 30,
        "w": 30,
        "x": 40,
        "y": 40
      },
      "kind": "rect"
    }
  },
  "layer_id": "layer_default",
  "object_id": "box"
}
```

## `draw_stroke`

```json
{
  "data": {},
  "layer_id": "L1"
}
```

## `draw_text`

```json
{
  "data": {},
  "layer_id": "L1"
}
```

## `duplicate_layer`

```json
{
  "layer_id": "L1"
}
```

## `erase`

```json
{
  "data": {},
  "layer_id": "L1"
}
```

## `estimate_dehaze`

```json
{}
```

## `export_png`

```json
{
  "layer_id": "L1",
  "max_edge": 512,
  "path": "example-export.png"
}
```

## `fill`

```json
{
  "data": {
    "color": {
      "a": 255,
      "b": 240,
      "g": 240,
      "r": 240
    }
  },
  "layer_id": "layer_default",
  "object_id": "bg"
}
```

## `find_atom`

```json
{
  "object_id": "s1"
}
```

## `get_changesets`

```json
{}
```

## `get_checkpoints`

```json
{}
```

## `get_diff`

```json
{
  "from_seq": 0
}
```

## `get_document`

```json
{}
```

## `get_log`

```json
{}
```

## `get_object`

```json
{
  "include_history": true,
  "object_id": "o1"
}
```

## `get_preferences`

```json
{}
```

## `get_state`

```json
{
  "include_objects": true
}
```

## `gradient_fill`

```json
{
  "angle": 0,
  "from": {
    "a": 255,
    "b": 0,
    "g": 0,
    "r": 255
  },
  "kind": "linear",
  "layer_id": "L1",
  "to": {
    "a": 255,
    "b": 255,
    "g": 0,
    "r": 0
  }
}
```

## `import_image`

```json
{
  "bitmap": {},
  "layer_id": "L1",
  "region": {
    "h": 32,
    "w": 32,
    "x": 0,
    "y": 0
  }
}
```

## `list_annotations`

```json
{}
```

## `list_assets`

```json
{
  "kind": "brush",
  "tag": "fur"
}
```

## `list_comments`

```json
{}
```

## `list_documents`

```json
{}
```

## `list_effects`

```json
{}
```

## `list_layers`

```json
{}
```

## `list_objects`

```json
{}
```

## `list_palette_colors`

```json
{
  "palette": "open-color.json"
}
```

## `list_selections`

```json
{}
```

## `list_stashes`

```json
{}
```

## `list_textures`

```json
{}
```

## `lock_layer`

```json
{
  "layer_id": "layer_default"
}
```

## `medium_stroke`

```json
{
  "layer_id": "L1",
  "medium": "oil",
  "points": [
    [
      60,
      220,
      1
    ],
    [
      140,
      220,
      1
    ]
  ]
}
```

## `new_document`

```json
{
  "background": {},
  "doc_id": "demo",
  "height": 640,
  "width": 900
}
```

## `render_region`

```json
{
  "include_image": true,
  "region": [
    0,
    0,
    64,
    64
  ]
}
```

## `unlock_layer`

```json
{
  "layer_id": "layer_default"
}
```

## `update_layer`

```json
{
  "layer_id": "layer_default",
  "patch": {
    "blend_mode": "multiply"
  }
}
```
