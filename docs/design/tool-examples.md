# 每个工具的可复制调用示例

**这份文件是生成出来的**（`scripts/tool-examples-doc.mjs`）—— 内容取自运行中服务端的 `GET /api/tools`，
所以它不会与实现漂移；`--check` 模式会在文档过期时失败。

当前共 222 个工具带示例；**这些示例是否真能跑通，由 `scripts/tool-example-acceptance.mjs` 判定**（本文件只保证与工具目录一致）。

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
  "layer_id": "layer_default",
  "adjustment_type": "invert"
}
```

## `add_filter`

```json
{
  "layer_id": "layer_default",
  "filter_name": "invert"
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
    "x": 0,
    "y": 0,
    "w": 100,
    "h": 100
  }
}
```

## `apply_adjustments`

```json
{
  "layer_id": "layer_default",
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
  ]
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
  "preset_id": "hong_kong",
  "layer_id": "layer_default",
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

## `bake_document`

```json
{}
```

## `bake_layer`

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
      "tool": "draw_stroke",
      "arguments": {
        "layer_id": "L1",
        "data": {}
      }
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
  "size": 24,
  "hardness": 0.6,
  "opacity": 0.8,
  "color": {
    "r": 0,
    "g": 64,
    "b": 255,
    "a": 255
  }
}
```

## `brush_stroke`

```json
{
  "brush": "100%_Opaque",
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
  "color": {
    "r": 255,
    "g": 0,
    "b": 0,
    "a": 255
  },
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
  "object_ids": [
    "shape_a",
    "shape_b"
  ],
  "mode": "union",
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
    "tool": "begin_transaction",
    "arguments": {}
  },
  {
    "tool": "draw_stroke",
    "arguments": {
      "layer_id": "L1",
      "data": {}
    }
  },
  {
    "tool": "commit_transaction",
    "arguments": {}
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
  "type": "region",
  "intent": "add",
  "target": {
    "target": "region",
    "bbox": {
      "x": 10,
      "y": 10,
      "w": 60,
      "h": 60
    }
  }
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
    "kind": "rect",
    "x": 100,
    "y": 0,
    "w": 100,
    "h": 300
  }
}
```

## `create_range_mask`

```json
{
  "mask_id": "m1",
  "layer_id": "L1",
  "range": "luminance",
  "low": 0,
  "high": 0.4
}
```

## `create_selection`

```json
{
  "selection_id": "sel1",
  "shape": {
    "kind": "rect",
    "bbox": {
      "x": 10,
      "y": 10,
      "w": 60,
      "h": 60
    }
  }
}
```

## `crop_canvas`

```json
{
  "region": {
    "x": 10,
    "y": 10,
    "w": 100,
    "h": 50
  }
}
```

## `declare_head`

```json
{
  "base_type": "atom",
  "base_id": "atom1"
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
  "mode": "gray"
}
```

## `draw_shape`

```json
{
  "layer_id": "layer_default",
  "object_id": "box",
  "data": {
    "geometry": {
      "kind": "rect",
      "bbox": {
        "x": 40,
        "y": 40,
        "w": 30,
        "h": 30
      }
    },
    "color": [
      200,
      60,
      60,
      255
    ]
  }
}
```

## `draw_stroke`

```json
{
  "layer_id": "L1",
  "data": {}
}
```

## `draw_text`

```json
{
  "layer_id": "L1",
  "data": {}
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
  "object_id": "p1",
  "op": "move_node",
  "subpath": 0,
  "node": 1,
  "to": [
    120,
    80
  ]
}
```

## `erase`

```json
{
  "layer_id": "L1",
  "data": {
    "region": {
      "x": 40,
      "y": 40,
      "w": 120,
      "h": 80
    },
    "size": 32,
    "hardness": 1,
    "opacity": 1
  }
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
  "doc_ids": [
    "page-01",
    "page-02"
  ],
  "dir": "book-pages"
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
  "path": "x"
}
```

## `extend_canvas`

```json
{
  "margins": {
    "left": 10,
    "top": 10,
    "right": 10,
    "bottom": 10
  }
}
```

## `fill`

```json
{
  "layer_id": "L1",
  "data": {}
}
```

## `fill_region`

```json
{
  "layer_id": "L1",
  "shape": {
    "type": "rect",
    "x": 10,
    "y": 10,
    "w": 60,
    "h": 60
  },
  "color": {
    "r": 200,
    "g": 30,
    "b": 60,
    "a": 255
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
  "layer_id": "L1",
  "mask": {}
}
```

## `flip_canvas`

```json
{
  "direction": "horizontal",
  "anchor_name": "center"
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

## `get_bake_status`

```json
{}
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
{}
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
  "layer_id": "layer_default",
  "from": {
    "x": 40,
    "y": 100,
    "color": "#2040a0"
  },
  "to": {
    "x": 260,
    "y": 100,
    "color": "#f0e0c0"
  },
  "brush": "classic-brush",
  "size": 24,
  "steps": 10
}
```

## `gradient_fill`

```json
{
  "layer_id": "L1",
  "from": {},
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
  "layer_id": "L1",
  "bitmap": {},
  "region": {
    "x": 0,
    "y": 0,
    "w": 32,
    "h": 32
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
  "layer_id": "layer_default",
  "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
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
  "layer_id": "L1",
  "points": [
    [
      10,
      10
    ]
  ],
  "direction": [
    10,
    0
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
  "limit": 10,
  "label": "nose"
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
  "width": 1,
  "height": 1
}
```

## `paint_mask`

```json
{
  "mask_id": "m1",
  "mode": "paint",
  "size": 60,
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
  ]
}
```

## `patch`

```json
{
  "layer_id": "L1",
  "source_region": {
    "x": 0,
    "y": 0,
    "w": 20,
    "h": 20
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
  "op": "reverse",
  "object_id": "L1"
}
```

## `preview_preset`

```json
{
  "preset_id": "hong_kong",
  "strength": 0.6,
  "knobs": {
    "teal": 0.3
  }
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

## `rebackground`

```json
{
  "color": {
    "r": 30,
    "g": 60,
    "b": 120,
    "a": 255
  }
}
```

## `recover_baked_blobs`

```json
{
  "layer_id": "L1"
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

## `release_document`

```json
{
  "document_id": "sample-oil"
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
  "region": [
    0,
    0,
    64,
    64
  ],
  "include_image": true
}
```

## `reopen_document`

```json
{
  "document_id": "sample-oil"
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

## `reorder_objects`

```json
{
  "layer_id": "L1",
  "order": [
    "o1"
  ]
}
```

## `replace_object_data`

```json
{
  "object_id": "L1",
  "data": {}
}
```

## `resample`

```json
{
  "object_id": "L1"
}
```

## `resize_canvas_bounds`

```json
{
  "width": 200,
  "height": 100
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
    "tool": "draw_stroke",
    "arguments": {
      "layer_id": "L1",
      "data": {}
    }
  },
  {
    "tool": "resolve_conflict",
    "arguments": {
      "resolution": "keep_ours"
    }
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
    "tool": "begin_changeset",
    "arguments": {}
  },
  {
    "tool": "commit_changeset",
    "arguments": {}
  },
  {
    "tool": "revert_changeset",
    "arguments": {
      "changeset_id": "cs1"
    }
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

## `revoke_token`

```json
{
  "token": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}
```

## `rotate_canvas`

```json
{
  "degrees": 90,
  "anchor_name": "center"
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
  "name": "my.gpl",
  "colors": [
    {
      "r": 255,
      "g": 0,
      "b": 0,
      "a": 255
    }
  ]
}
```

## `scale_canvas`

```json
{
  "scale": 2,
  "anchor_name": "center"
}
```

## `scatter_strokes`

```json
{
  "layer_id": "L1",
  "seed": 1,
  "area": {
    "x": 0,
    "y": 0,
    "w": 100,
    "h": 100
  },
  "palette": [
    {
      "r": 255,
      "g": 0,
      "b": 0,
      "a": 255
    }
  ],
  "brush": "100%_Opaque"
}
```

## `select_opaque_pixels`

```json
{
  "layer_id": "L1"
}
```

## `set_anchor`

```json
{
  "name": "x",
  "x": 1,
  "y": 1
}
```

## `set_brush_curve`

```json
{
  "brush": "100%_Opaque",
  "setting": "hardness",
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
  ]
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
  "group_id": "g1",
  "delta": {
    "dx": 10,
    "dy": 0
  }
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
  "layer_id": "layer_default",
  "key": "visible",
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
  "shape": {},
  "color_a": [],
  "color_b": [],
  "layer_id": "L1"
}
```

## `smart_edge_soften`

```json
{
  "layer_id": "L1",
  "shape": {
    "type": "rect",
    "x": 30,
    "y": 30,
    "w": 120,
    "h": 100
  }
}
```

## `smart_erase_shape`

```json
{
  "layer_id": "L1",
  "shape": {}
}
```

## `smart_fill`

```json
{
  "layer_id": "L1",
  "shape": {
    "type": "rect",
    "x": 40,
    "y": 40,
    "w": 180,
    "h": 120
  },
  "brush": "2B_pencil",
  "color": [
    0.86,
    0.24,
    0.16,
    1
  ]
}
```

## `smart_glaze`

```json
{
  "shape": {},
  "color": [],
  "layer_id": "L1"
}
```

## `smart_hatch_follow`

```json
{
  "layer_id": "L1",
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
  "color": [
    0.2,
    0.15,
    0.1,
    1
  ],
  "width": 12,
  "size": 22,
  "density": 0.6
}
```

## `smart_sample`

```json
{
  "ref": "L_ref",
  "layer_id": "L1",
  "shape": {
    "type": "rect",
    "x": 40,
    "y": 40,
    "w": 180,
    "h": 120
  },
  "samples": 8
}
```

## `smart_select`

```json
{
  "seed": [
    320,
    240
  ],
  "tolerance": 0.2,
  "mode": "new"
}
```

## `smart_texture`

```json
{
  "layer_id": "L1",
  "shape": {
    "kind": "rect",
    "x": 20,
    "y": 20,
    "w": 360,
    "h": 360
  },
  "texture_type": "stone",
  "density": 0.5
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

## `stroke_heatmap`

```json
{
  "region": {
    "x": 0,
    "y": 0,
    "w": 400,
    "h": 300
  }
}
```

## `stroke_path`

```json
{
  "layer_id": "L1",
  "subpaths": [
    {
      "start": [
        60,
        200
      ],
      "segments": [
        {
          "op": "C",
          "c1": [
            160,
            60
          ],
          "c2": [
            300,
            60
          ],
          "to": [
            400,
            200
          ]
        }
      ]
    }
  ],
  "brush": "2B_pencil",
  "size": 24,
  "color": {
    "r": 30,
    "g": 30,
    "b": 30,
    "a": 255
  }
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
      "tool": "add_filter",
      "arguments": {
        "layer_id": "layer_default",
        "filter_name": "invert"
      }
    }
  ],
  "summary": "把画面反相"
}
```

## `swatch_brush_strokes`

```json
{
  "brushes": [
    "Classic_Paint"
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

## `toned_ground`

```json
{
  "color": {
    "r": 210,
    "g": 190,
    "b": 160,
    "a": 255
  }
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
  "selection_id": "s1",
  "translate": [
    10,
    0
  ],
  "scale": [
    1.5,
    1.5
  ],
  "rotate_deg": 30,
  "pivot": [
    100,
    100
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
  "layer_id": "L1",
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
    "tool": "create_layer",
    "arguments": {
      "layer_id": "v1",
      "kind": "vector"
    }
  },
  {
    "tool": "draw_stroke",
    "arguments": {
      "layer_id": "v1",
      "data": {
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
        "size": 20,
        "color": [
          200,
          60,
          40,
          255
        ]
      }
    }
  },
  {
    "tool": "vector_erase",
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
    }
  }
]
```

## `vector_hit_test`

```json
{
  "layer_id": "v1",
  "x": 80,
  "y": 60,
  "tolerance": 2
}
```

## `verify_package`

```json
{
  "path": "exports/demo.yanshi"
}
```
