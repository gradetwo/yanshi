# 每个工具的可复制调用示例

**这份文件是生成出来的**（`scripts/tool-examples-doc.mjs`）—— 内容取自运行中服务端的 `GET /api/tools`，
所以它不会与实现漂移；`--check` 模式会在文档过期时失败。

当前共 135 个工具带示例；**这些示例是否真能跑通，由 `scripts/tool-example-acceptance.mjs` 判定**（本文件只保证与工具目录一致）。

## `abort_changeset`

```json
{}
```

## `accept_suggestion`

```json
{
  "suggestion_id": "sug1"
}
```

## `accept_suggestions`

```json
{
  "suggestion_ids": [
    "sug1"
  ]
}
```

## `add_adjustment`

```json
{
  "adjustment_type": "invert",
  "layer_id": "layer_default"
}
```

## `add_filter`

```json
{
  "filter_name": "invert",
  "layer_id": "layer_default"
}
```

## `add_to_group`

```json
{
  "group_id": "g1",
  "object_id": "L1"
}
```

## `analyze_region`

```json
{
  "region": {
    "h": 100,
    "w": 100,
    "x": 0,
    "y": 0
  }
}
```

## `apply_stash`

```json
{
  "stash_id": "st1"
}
```

## `batch`

```json
{
  "calls": [
    {
      "arguments": {
        "data": {},
        "layer_id": "L1"
      },
      "tool": "draw_stroke"
    }
  ]
}
```

## `begin_changeset`

```json
{}
```

## `begin_transaction`

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

## `cancel_job`

```json
{
  "job_id": "job1"
}
```

## `checkpoint`

```json
{}
```

## `clear_reference`

```json
{}
```

## `clone_stamp`

```json
{
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ],
    [
      50,
      50
    ]
  ],
  "source_offset": [
    0,
    -30
  ]
}
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

## `commit_changeset`

```json
{}
```

## `commit_transaction`

```json
[
  {
    "arguments": {},
    "tool": "begin_transaction"
  },
  {
    "arguments": {
      "data": {},
      "layer_id": "L1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {},
    "tool": "commit_transaction"
  }
]
```

## `convert_to_path`

```json
{
  "object_id": "L1"
}
```

## `convert_to_shape`

```json
{
  "object_id": "L1"
}
```

## `create_annotation`

```json
{
  "intent": "add",
  "target": {
    "bbox": {
      "h": 60,
      "w": 60,
      "x": 10,
      "y": 10
    },
    "target": "region"
  },
  "type": "region"
}
```

## `create_group`

```json
{
  "group_id": "x",
  "layer_id": "L1"
}
```

## `create_instance`

```json
{
  "instance_id": "inst1",
  "layer_id": "L1",
  "master_id": "L1"
}
```

## `create_layer`

```json
{
  "layer_id": "L1",
  "name": "Layer 1"
}
```

## `create_mask`

```json
{
  "mask_id": "m1",
  "shape": {
    "bbox": {
      "h": 60,
      "w": 60,
      "x": 10,
      "y": 10
    },
    "kind": "rect"
  }
}
```

## `create_selection`

```json
{
  "selection_id": "sel1",
  "shape": {
    "bbox": {
      "h": 60,
      "w": 60,
      "x": 10,
      "y": 10
    },
    "kind": "rect"
  }
}
```

## `declare_head`

```json
{
  "base_id": "atom1",
  "base_type": "atom"
}
```

## `delete_annotation`

```json
{
  "annotation_id": "ann1"
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

## `delete_selection`

```json
{
  "selection_id": "sel1"
}
```

## `detach_instance`

```json
{
  "instance_id": "inst1"
}
```

## `discard_stash`

```json
{
  "stash_id": "st1"
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

## `export_project`

```json
{
  "path": "exports/demo.yanshi"
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

## `fill_region`

```json
{
  "color": {
    "a": 255,
    "b": 60,
    "g": 30,
    "r": 200
  },
  "layer_id": "L1",
  "shape": {
    "h": 60,
    "type": "rect",
    "w": 60,
    "x": 10,
    "y": 10
  }
}
```

## `find_atom`

```json
{
  "object_id": "s1"
}
```

## `get_ancestors`

```json
[
  {
    "arguments": {
      "data": {},
      "layer_id": "L1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {
      "object_id": "L1"
    },
    "tool": "get_ancestors"
  }
]
```

## `get_annotation`

```json
{
  "annotation_id": "ann1"
}
```

## `get_atom`

```json
{
  "atom_id": "atom1"
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

## `get_dependency_graph`

```json
[
  {
    "arguments": {
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
      "object_id": "p0"
    },
    "tool": "draw_shape"
  },
  {
    "arguments": {
      "object_id": "p0"
    },
    "tool": "get_dependency_graph"
  }
]
```

## `get_descendants`

```json
[
  {
    "arguments": {
      "data": {},
      "layer_id": "L1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {
      "object_id": "L1"
    },
    "tool": "get_descendants"
  }
]
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

## `get_job`

```json
{
  "job_id": "job1"
}
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

## `get_object_history`

```json
[
  {
    "arguments": {
      "data": {},
      "layer_id": "L1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {
      "object_id": "L1"
    },
    "tool": "get_object_history"
  }
]
```

## `get_preferences`

```json
{}
```

## `get_render_status`

```json
{
  "atom_id": "atom1"
}
```

## `get_resolved_state`

```json
[
  {
    "arguments": {
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
      "object_id": "p1"
    },
    "tool": "draw_shape"
  },
  {
    "arguments": {
      "object_id": "p1"
    },
    "tool": "get_resolved_state"
  }
]
```

## `get_state`

```json
{
  "include_objects": true
}
```

## `gradient_blend`

```json
{
  "brush": "classic-brush",
  "from": {
    "color": "#2040a0",
    "x": 40,
    "y": 100
  },
  "layer_id": "layer_default",
  "size": 24,
  "steps": 10,
  "to": {
    "color": "#f0e0c0",
    "x": 260,
    "y": 100
  }
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

## `heal_stamp`

```json
{
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ],
    [
      50,
      50
    ]
  ],
  "source_offset": [
    0,
    -30
  ]
}
```

## `import_asset`

```json
{
  "kind": "brush",
  "name": "my.myb"
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

## `import_project`

```json
{
  "path": "exports/demo.yanshi"
}
```

## `link_to_master`

```json
{
  "instance_id": "inst1",
  "master_id": "L1"
}
```

## `liquify_pinch`

```json
{
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ]
  ]
}
```

## `liquify_push`

```json
{
  "direction": [
    10,
    0
  ],
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ]
  ]
}
```

## `liquify_twirl`

```json
{
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ]
  ]
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

## `list_brushes`

```json
{}
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

## `list_suggestions`

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

## `move_object`

```json
{
  "object_id": "L1"
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

## `patch`

```json
{
  "layer_id": "L1",
  "source_region": {
    "h": 20,
    "w": 20,
    "x": 0,
    "y": 0
  },
  "target": [
    40,
    40
  ]
}
```

## `path_edit`

```json
{
  "object_id": "L1",
  "op": "reverse"
}
```

## `preview_suggestion`

```json
{}
```

## `reapply`

```json
{
  "atom_id": "atom1"
}
```

## `redo_last`

```json
[
  {
    "arguments": {
      "data": {},
      "layer_id": "L1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {
      "count": 1
    },
    "tool": "undo_last"
  },
  {
    "arguments": {
      "count": 1
    },
    "tool": "redo_last"
  }
]
```

## `reject_annotation`

```json
{
  "annotation_id": "ann1"
}
```

## `reject_suggestion`

```json
{
  "suggestion_id": "sug1"
}
```

## `reject_suggestions`

```json
{
  "suggestion_ids": [
    "sug1"
  ]
}
```

## `remove_from_group`

```json
{
  "group_id": "g1",
  "object_id": "L1"
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

## `reorder_layers`

```json
{
  "order": [
    "L1"
  ]
}
```

## `replace_object_data`

```json
{
  "data": {},
  "object_id": "L1"
}
```

## `resample`

```json
{
  "object_id": "L1"
}
```

## `resolve_annotation`

```json
{
  "annotation_id": "ann1"
}
```

## `resolve_conflict`

```json
[
  {
    "arguments": {
      "data": {},
      "layer_id": "L1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {
      "resolution": "keep_ours"
    },
    "tool": "resolve_conflict"
  }
]
```

## `restore_checkpoint`

```json
{
  "checkpoint_id": "cp1"
}
```

## `restore_object`

```json
{
  "object_id": "L1"
}
```

## `revert`

```json
{
  "atom_id": "atom1"
}
```

## `revert_changeset`

```json
[
  {
    "arguments": {},
    "tool": "begin_changeset"
  },
  {
    "arguments": {},
    "tool": "commit_changeset"
  },
  {
    "arguments": {
      "changeset_id": "cs1"
    },
    "tool": "revert_changeset"
  }
]
```

## `revert_to`

```json
{
  "atom_id": "atom1"
}
```

## `sample_color`

```json
{
  "x": 120,
  "y": 64
}
```

## `save_palette`

```json
{
  "colors": [
    {
      "a": 255,
      "b": 0,
      "g": 0,
      "r": 255
    }
  ],
  "name": "my.gpl"
}
```

## `scatter_strokes`

```json
{
  "area": {
    "h": 100,
    "w": 100,
    "x": 0,
    "y": 0
  },
  "brush": "100%_Opaque",
  "layer_id": "L1",
  "palette": [
    {
      "a": 255,
      "b": 0,
      "g": 0,
      "r": 255
    }
  ],
  "seed": 1
}
```

## `set_brush_dynamics`

```json
{
  "brush": "100%_Opaque",
  "curve": {
    "size_pressure": [
      [
        0,
        0.2
      ],
      [
        1,
        1
      ]
    ]
  }
}
```

## `set_group_transform`

```json
{
  "delta": {
    "dx": 10,
    "dy": 0
  },
  "group_id": "g1"
}
```

## `set_layer_blend`

```json
{
  "layer_id": "L1",
  "mode": "multiply"
}
```

## `set_preferences`

```json
{
  "values": {
    "reference.blob_hash": null
  }
}
```

## `set_property`

```json
{
  "key": "visible",
  "layer_id": "layer_default",
  "value": true
}
```

## `smudge`

```json
{
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ]
  ]
}
```

## `submit_offline`

```json
{
  "atoms": [
    {
      "kind": "draw_stroke",
      "payload": {}
    }
  ]
}
```

## `suggest`

```json
{
  "patch": [
    {
      "arguments": {
        "filter_name": "invert",
        "layer_id": "layer_default"
      },
      "tool": "add_filter"
    }
  ],
  "summary": "把画面反相"
}
```

## `texture_background`

```json
{
  "texture": "Paper001.png"
}
```

## `transform_object`

```json
{
  "object_id": "L1"
}
```

## `undo_last`

```json
[
  {
    "arguments": {
      "data": {},
      "layer_id": "L1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {
      "count": 1
    },
    "tool": "undo_last"
  }
]
```

## `unlock_layer`

```json
{
  "layer_id": "layer_default"
}
```

## `update_adjustment`

```json
{
  "object_id": "L1",
  "params": {}
}
```

## `update_annotation`

```json
{
  "annotation_id": "ann1"
}
```

## `update_filter`

```json
{
  "object_id": "L1",
  "params": {}
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

## `update_object`

```json
{
  "object_id": "L1",
  "patch": {}
}
```

## `update_override`

```json
{
  "instance_id": "inst1"
}
```

## `update_stroke`

```json
{
  "object_id": "L1"
}
```

## `update_sync_policy`

```json
{
  "instance_id": "inst1",
  "policy": "all"
}
```
