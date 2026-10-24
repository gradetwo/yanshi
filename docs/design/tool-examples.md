# 每个工具的可复制调用示例

**这份文件是生成出来的**（`scripts/tool-examples-doc.mjs`）—— 内容取自运行中服务端的 `GET /api/tools`，
所以它不会与实现漂移；`--check` 模式会在文档过期时失败。

当前共 203 个工具带示例；**这些示例是否真能跑通，由 `scripts/tool-example-acceptance.mjs` 判定**（本文件只保证与工具目录一致）。

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

## `apply_adjustments`

```json
{
  "adjustments": [
    {
      "adjustment_type": "exposure",
      "params": {
        "ev": 0.3
      }
    },
    {
      "adjustment_type": "vibrance",
      "params": {
        "amount": 0.6
      }
    }
  ],
  "layer_id": "layer_default"
}
```

## `apply_curve_preset`

```json
{
  "brush": "100%_Opaque",
  "shape": "ease-in"
}
```

## `apply_preset`

```json
{
  "layer_id": "layer_default",
  "preset_id": "hong_kong",
  "strength": 0.8
}
```

## `apply_stash`

```json
{
  "stash_id": "st1"
}
```

## `auto_retouch`

```json
{
  "layer_id": "L1"
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

## `calibrate`

```json
{}
```

## `cancel_job`

```json
{
  "job_id": "job1"
}
```

## `cancel_operation`

```json
{}
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

## `collect_diagnostics`

```json
{
  "include_thumbnail": false
}
```

## `collect_garbage`

```json
{}
```

## `combine_shapes`

```json
{
  "mode": "union",
  "object_ids": [
    "shape_a",
    "shape_b"
  ],
  "result_id": "merged"
}
```

## `comment`

```json
{
  "text": "x"
}
```

## `commit_batch`

```json
{
  "attempt_ids": [
    "a1",
    "a2"
  ]
}
```

## `commit_changeset`

```json
{}
```

## `commit_stroke`

```json
{
  "attempt_id": "abc123"
}
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
    "h": 300,
    "kind": "rect",
    "w": 100,
    "x": 100,
    "y": 0
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

## `delete_anchor`

```json
{
  "name": "left_eye"
}
```

## `delete_annotation`

```json
{
  "annotation_id": "ann1"
}
```

## `delete_document`

```json
{
  "document_id": "sample-oil"
}
```

## `delete_layer`

```json
{
  "layer_id": "L1"
}
```

## `delete_mask`

```json
{
  "mask_id": "m1"
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

## `describe_canvas`

```json
{}
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

## `discard_stroke`

```json
{
  "attempt_id": "abc123"
}
```

## `dodge_burn_setup`

```json
{
  "layer_id": "L1",
  "mode": "curves"
}
```

## `draw_shape`

```json
{
  "data": {
    "color": [
      200,
      60,
      60,
      255
    ],
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

## `edit_path`

```json
{
  "node": 1,
  "object_id": "p1",
  "op": "move_node",
  "subpath": 0,
  "to": [
    120,
    80
  ]
}
```

## `erase`

```json
{
  "data": {
    "hardness": 1,
    "opacity": 1,
    "region": {
      "h": 80,
      "w": 120,
      "x": 40,
      "y": 40
    },
    "size": 32
  },
  "layer_id": "L1"
}
```

## `estimate_dehaze`

```json
{}
```

## `explain_tool`

```json
{
  "name": "stroke_path"
}
```

## `export_book_pngs`

```json
{
  "dir": "book-pages",
  "doc_ids": [
    "page-01",
    "page-02"
  ]
}
```

## `export_png`

```json
{
  "path": "x"
}
```

## `export_project`

```json
{
  "path": "exports/demo.yanshi"
}
```

## `export_psd_layers`

```json
{
  "filename": "yanshi-export.psd"
}
```

## `export_svg`

```json
{
  "path": "x"
}
```

## `export_timelapse`

```json
{
  "fps": 30,
  "frame_every": 10,
  "path": "process.mp4",
  "width": 2048
}
```

## `fill`

```json
{
  "data": {},
  "layer_id": "L1"
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

## `fit_background_field`

```json
{
  "degree": 2,
  "layer_id": "L1",
  "mask": {
    "h": 140,
    "type": "rect",
    "w": 160,
    "x": 120,
    "y": 80
  }
}
```

## `frequency_separation`

```json
{
  "layer_id": "L1",
  "radius": 8
}
```

## `get_ancestors`

```json
{
  "object_id": "o1"
}
```

## `get_anchors`

```json
{}
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

## `get_brush_dynamics`

```json
{
  "brush": "100%_Opaque"
}
```

## `get_brush_settings`

```json
{
  "brush": "Clouds"
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
{
  "object_id": "o1"
}
```

## `get_descendants`

```json
{
  "object_id": "o1"
}
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

## `get_histogram`

```json
{
  "region": {
    "h": 150,
    "w": 200,
    "x": 0,
    "y": 0
  }
}
```

## `get_inflight`

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
  "object_id": "o1"
}
```

## `get_object_history`

```json
{
  "object_id": "o1"
}
```

## `get_path`

```json
{
  "object_id": "p1"
}
```

## `get_preferences`

```json
{}
```

## `get_preset`

```json
{
  "preset_id": "hong_kong"
}
```

## `get_render_status`

```json
{
  "atom_id": "atom1"
}
```

## `get_resolved_state`

```json
{
  "object_id": "o1"
}
```

## `get_state`

```json
{
  "include_objects": true
}
```

## `get_stroke_log`

```json
{
  "limit": 10
}
```

## `get_undo_status`

```json
{}
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
  "from": {},
  "layer_id": "L1",
  "to": {}
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

## `import_psd`

```json
{
  "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
  "layer_id": "layer_default"
}
```

## `import_psd_layers`

```json
{
  "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
  "name_prefix": "PSD·"
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

## `list_atoms`

```json
{
  "label": "nose",
  "limit": 10
}
```

## `list_attempts`

```json
{
  "limit": 10,
  "status": "pending"
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

## `list_engine_capabilities`

```json
{}
```

## `list_fonts`

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

## `list_presets`

```json
{
  "tag": "夜景"
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
  "height": 1,
  "width": 1
}
```

## `paint_mask`

```json
{
  "mask_id": "m1",
  "mode": "paint",
  "points": [
    [
      150,
      20,
      1
    ],
    [
      150,
      280,
      1
    ]
  ],
  "size": 60
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

## `preview_preset`

```json
{
  "knobs": {
    "teal": 0.3
  },
  "preset_id": "hong_kong",
  "strength": 0.6
}
```

## `preview_suggestion`

```json
{}
```

## `put_blob`

```json
{
  "path": "/tmp/red8.png"
}
```

## `reapply`

```json
{
  "atom_id": "atom1"
}
```

## `redo_last`

```json
{}
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

## `revert_atoms`

```json
{
  "atom_ids": [
    "a1",
    "a2"
  ]
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

## `revert_group`

```json
{
  "group_id": "g-2026-10-19-1"
}
```

## `revert_to`

```json
{
  "atom_id": "atom1"
}
```

## `revert_to_seq`

```json
{
  "seq": 100
}
```

## `sample_color`

```json
{
  "x": 120,
  "y": 64
}
```

## `sample_colors`

```json
{
  "points": [
    [
      120,
      64
    ],
    [
      200,
      80
    ]
  ]
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

## `select_opaque_pixels`

```json
{
  "binarize": false,
  "close_pinholes": true,
  "layer_id": "L1",
  "min_alpha": 0.5,
  "selection_id": "s_opaque"
}
```

## `set_anchor`

```json
{
  "name": "left_eye",
  "note": "外眼角偏 2px",
  "x": 268,
  "y": 286
}
```

## `set_brush_curve`

```json
{
  "brush": "100%_Opaque",
  "input": "pressure",
  "points": [
    [
      0,
      -0.2
    ],
    [
      1,
      0.2
    ]
  ],
  "setting": "hardness"
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

## `set_brush_setting`

```json
{
  "brush": "Clouds",
  "setting": "smudge",
  "value": 0.5
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
  "values": {}
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

## `set_reference`

```json
{
  "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
}
```

## `smart_blend`

```json
{
  "color_a": [
    0.86,
    0.24,
    0.16,
    1
  ],
  "color_b": [
    0.16,
    0.24,
    0.86,
    1
  ],
  "layer_id": "L1",
  "shape": {
    "h": 80,
    "type": "rect",
    "w": 120,
    "x": 40,
    "y": 40
  }
}
```

## `smart_edge_soften`

```json
{
  "layer_id": "L1",
  "shape": {
    "h": 100,
    "type": "rect",
    "w": 120,
    "x": 30,
    "y": 30
  }
}
```

## `smart_erase_shape`

```json
{
  "layer_id": "L1",
  "shape": {
    "points": [
      [
        100,
        100
      ],
      [
        200,
        100
      ],
      [
        150,
        180
      ]
    ],
    "type": "polygon"
  }
}
```

## `smart_fill`

```json
{
  "brush": "2B_pencil",
  "color": [
    0.86,
    0.24,
    0.16,
    1
  ],
  "layer_id": "L1",
  "shape": {
    "h": 120,
    "type": "rect",
    "w": 180,
    "x": 40,
    "y": 40
  }
}
```

## `smart_glaze`

```json
{
  "color": [
    0.86,
    0.24,
    0.16,
    1
  ],
  "layer_id": "L1",
  "shape": {
    "h": 60,
    "type": "rect",
    "w": 100,
    "x": 60,
    "y": 60
  }
}
```

## `smart_hatch_follow`

```json
{
  "color": [
    0.2,
    0.15,
    0.1,
    1
  ],
  "density": 0.6,
  "guides": [
    [
      [
        40,
        200
      ],
      [
        120,
        160
      ],
      [
        200,
        200
      ]
    ]
  ],
  "layer_id": "L1",
  "size": 22,
  "width": 12
}
```

## `smart_sample`

```json
{
  "layer_id": "L1",
  "ref": "L_ref",
  "samples": 8,
  "shape": {
    "h": 120,
    "type": "rect",
    "w": 180,
    "x": 40,
    "y": 40
  }
}
```

## `smart_select`

```json
{
  "mode": "new",
  "seed": [
    320,
    240
  ],
  "tolerance": 0.2
}
```

## `smart_texture`

```json
{
  "density": 0.5,
  "layer_id": "L1",
  "shape": {
    "h": 360,
    "kind": "rect",
    "w": 360,
    "x": 20,
    "y": 20
  },
  "texture_type": "stone"
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

## `stroke_path`

```json
{
  "brush": "2B_pencil",
  "color": {
    "a": 255,
    "b": 30,
    "g": 30,
    "r": 30
  },
  "layer_id": "L1",
  "size": 24,
  "subpaths": [
    {
      "segments": [
        {
          "c1": [
            160,
            60
          ],
          "c2": [
            300,
            60
          ],
          "op": "C",
          "to": [
            400,
            200
          ]
        }
      ],
      "start": [
        60,
        200
      ]
    }
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

## `swatch_brush_strokes`

```json
{
  "brushes": [
    "Blender"
  ],
  "size": 24
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

## `transform_selection`

```json
{
  "pivot": [
    100,
    100
  ],
  "rotate_deg": 30,
  "scale": [
    1.5,
    1.5
  ],
  "selection_id": "s1",
  "translate": [
    10,
    0
  ]
}
```

## `try_batch`

```json
{
  "layer_id": "L1",
  "strokes": [
    {
      "brush": "2B_pencil",
      "points": [
        [
          10,
          10
        ],
        [
          40,
          40
        ]
      ]
    }
  ]
}
```

## `try_stroke`

```json
{
  "brush": "2B_pencil",
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ],
    [
      40,
      40
    ]
  ],
  "size": 8
}
```

## `undo_last`

```json
{}
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

## `vector_erase`

```json
[
  {
    "arguments": {
      "kind": "vector",
      "layer_id": "v1"
    },
    "tool": "create_layer"
  },
  {
    "arguments": {
      "data": {
        "color": [
          200,
          60,
          40,
          255
        ],
        "points": [
          [
            20,
            60
          ],
          [
            140,
            60
          ]
        ],
        "size": 20
      },
      "layer_id": "v1"
    },
    "tool": "draw_stroke"
  },
  {
    "arguments": {
      "layer_id": "v1",
      "points": [
        [
          80,
          40
        ],
        [
          80,
          80
        ]
      ],
      "radius": 6
    },
    "tool": "vector_erase"
  }
]
```

## `vector_hit_test`

```json
{
  "layer_id": "v1",
  "tolerance": 2,
  "x": 80,
  "y": 60
}
```
