//! 工具协议层（设计文档 10 章）。
//!
//! - **10.1 成功返回格式**：`{ok, atom_id, seq, changeset_id, head, dirty_bbox, preview, job_id, warnings, suggestions}`。
//! - **10.2 工具集与暴露分层**：核心层 27 个默认注册；扩展组按 `profile` 启用，命名前缀分组。
//! - **5.7 错误协议**：失败返回 `{ok:false, error_code, retryable, context}`。
//! - **6.7 Job**：重型/语义工具返回 `job_id`；`wait_for_render`（默认 true，500ms）超时后返回
//!   `job_pending` 由 Agent 轮询。
//!
//! 工具层只做「参数解析 + 构造原子 + 提交 + 组织返回」，语义全部落在
//! [`Document`] / [`Workspace`] 与折叠引擎上。

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use yanshi_core::{
    Atom, AtomKind, Bbox, Changeset, ChangesetId, DocumentState, ErrorCode, ErrorContext, HeadBase,
    ObjectType, Result, YanshiError,
};

use crate::annotations::{
    AnnotationFilter, AnnotationIntent, AnnotationStatus, AnnotationTarget, AnnotationType,
    NewAnnotation,
};
use crate::base64;
use crate::document::{CommitResult, RenderedPreview};
use crate::job::JobStatus;
use crate::service::{DocThumbSize, Workspace};

/// 工具分层（10.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// 核心层：默认注册（27 个）。
    Core,
    /// 历史与检查点。
    History,
    /// 变更集与事务。
    Changeset,
    /// 修图。
    Retouch,
    /// 语义工具（外部模型服务）。
    Semantic,
    /// 冲突解决。
    Conflict,
    /// 标注。
    Annotation,
    /// 协作评论与建议。
    Collab,
    /// 结构修改（属性、路径、笔刷）。
    Structure,
}

impl Profile {
    /// 全部 profile。
    pub const ALL: [Profile; 9] = [
        Self::Core,
        Self::History,
        Self::Changeset,
        Self::Retouch,
        Self::Semantic,
        Self::Conflict,
        Self::Annotation,
        Self::Collab,
        Self::Structure,
    ];

    /// 字符串名（MCP/HTTP/WS 会话的 `profile` 参数取值）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::History => "history",
            Self::Changeset => "changeset",
            Self::Retouch => "retouch",
            Self::Semantic => "semantic",
            Self::Conflict => "conflict",
            Self::Annotation => "annotation",
            Self::Collab => "collab",
            Self::Structure => "structure",
        }
    }

    /// 由字符串解析。
    pub fn parse(text: &str) -> Result<Self> {
        Ok(match text {
            "core" => Self::Core,
            "history" => Self::History,
            "changeset" => Self::Changeset,
            "retouch" => Self::Retouch,
            "semantic" => Self::Semantic,
            "conflict" => Self::Conflict,
            "annotation" => Self::Annotation,
            "collab" => Self::Collab,
            "structure" => Self::Structure,
            other => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("未知 profile {other}")),
                ))
            }
        })
    }
}

/// 参数类型（用于生成 MCP inputSchema）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// 字符串。
    String,
    /// 整数。
    Integer,
    /// 浮点。
    Number,
    /// 布尔。
    Boolean,
    /// 对象。
    Object,
    /// 数组。
    Array,
    /// 任意 JSON 值（用于多态参数，如 `set_property.value` 随 `key` 变化）。
    Any,
}

impl ParamKind {
    /// JSON Schema 的 `type`。
    const fn json_type(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Object => "object",
            Self::Array => "array",
            Self::Any => "any",
        }
    }
}

/// 参数描述。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    /// 参数名。
    pub name: &'static str,
    /// 类型。
    pub kind: ParamKind,
    /// 是否必填。
    pub required: bool,
    /// 说明。
    pub description: &'static str,
}

/// 工具描述。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolSpec {
    /// 工具名（统一前缀分组，10.2）。
    pub name: &'static str,
    /// 所属 profile。
    pub profile: Profile,
    /// 一句话说明。
    pub summary: &'static str,
    /// 是否修改文档（只读工具不产生原子）。
    pub mutating: bool,
    /// 参数。
    pub params: &'static [ParamSpec],
}

macro_rules! param {
    ($name:literal, $kind:ident, $required:literal, $desc:literal) => {
        ParamSpec {
            name: $name,
            kind: ParamKind::$kind,
            required: $required,
            description: $desc,
        }
    };
}

/// 工具调用上下文。
pub struct ToolContext<'a> {
    /// 工作区。
    pub workspace: &'a mut Workspace,
    /// 目标文档。
    pub doc_id: String,
    /// 操作者（与鉴权绑定，12.7）。
    pub actor: String,
    /// 会话。
    pub session: String,
    /// 是否拥有 owner 权限（跨 actor revert，12.5）。
    pub owner: bool,
    /// 是否等待渲染完成（6.7 `wait_for_render`，默认 true）。
    pub wait_for_render: bool,
    /// 等待预算（毫秒）。
    pub wait_budget_ms: u64,
    /// 当前时间。
    pub now: i64,
    /// batch 内部使用的变更集 id（5.6）。
    changeset: Option<ChangesetId>,
}

impl<'a> ToolContext<'a> {
    /// 构造。
    pub fn new(
        workspace: &'a mut Workspace,
        doc_id: impl Into<String>,
        actor: impl Into<String>,
        session: impl Into<String>,
    ) -> Self {
        Self {
            workspace,
            doc_id: doc_id.into(),
            actor: actor.into(),
            session: session.into(),
            owner: false,
            wait_for_render: true,
            wait_budget_ms: 500,
            now: yanshi_core::now_ms(),
            changeset: None,
        }
    }

    /// 授予 owner 权限。
    pub fn with_owner(mut self, owner: bool) -> Self {
        self.owner = owner;
        self
    }

    /// 设置等待渲染策略。
    pub fn with_wait_for_render(mut self, wait: bool, budget_ms: u64) -> Self {
        self.wait_for_render = wait;
        self.wait_budget_ms = budget_ms;
        self
    }

    fn commit(&mut self, kind: AtomKind, payload: Value) -> Result<CommitResult> {
        let atom = Atom::new(kind, self.actor.clone(), self.session.clone(), payload);
        match self.changeset.clone() {
            // batch 内部：所有原子归属同一个变更集（5.6）。
            Some(changeset) => {
                let mut results = self.workspace.commit_changeset(
                    &self.doc_id,
                    vec![atom],
                    &self.actor,
                    self.owner,
                    changeset,
                )?;
                Ok(results.remove(0))
            }
            None => self
                .workspace
                .commit(&self.doc_id, atom, &self.actor, self.owner),
        }
    }

    fn commit_changeset(
        &mut self,
        atoms: Vec<Atom>,
        changeset_id: ChangesetId,
    ) -> Result<Vec<CommitResult>> {
        let changeset = self.changeset.clone().unwrap_or(changeset_id);
        self.workspace
            .commit_changeset(&self.doc_id, atoms, &self.actor, self.owner, changeset)
    }
}

/// 工具注册表。
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: Vec<ToolSpec>,
    profiles: BTreeSet<Profile>,
}

impl ToolRegistry {
    /// 核心层（默认注册，27 个）。
    pub fn core() -> Self {
        Self::with_profiles(&[Profile::Core])
    }

    /// 按 profile 启用工具集。
    pub fn with_profiles(profiles: &[Profile]) -> Self {
        let mut enabled: BTreeSet<Profile> = profiles.iter().copied().collect();
        enabled.insert(Profile::Core);
        let mut tools: Vec<ToolSpec> = ALL_TOOLS
            .iter()
            .copied()
            .filter(|tool| enabled.contains(&tool.profile))
            .collect();
        tools.sort_by_key(|tool| (tool.profile, tool.name));
        Self {
            tools,
            profiles: enabled,
        }
    }

    /// 全量工具（核心 + 全部扩展组）。
    pub fn full() -> Self {
        Self::with_profiles(&Profile::ALL)
    }

    /// 已注册工具。
    pub fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }

    /// 已启用的 profile。
    pub fn profiles(&self) -> &BTreeSet<Profile> {
        &self.profiles
    }

    /// 工具数量。
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// 按名查找。
    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.iter().find(|tool| tool.name == name)
    }

    /// MCP `tools/list` 用的 JSON Schema。
    pub fn input_schema(&self, name: &str) -> Option<Value> {
        let tool = self.get(name)?;
        Some(input_schema(tool))
    }

    /// 调用工具；返回 10.1 或 5.7 形状的 JSON。
    pub fn call(&self, ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
        let Some(spec) = self.get(name) else {
            return error_response(&YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("未知工具 {name}（当前 profile 未启用或不存在）")),
            ));
        };
        if let Err(error) = validate_args(spec, args) {
            return error_response(&error);
        }
        match dispatch(spec, ctx, args) {
            Ok(value) => ok_response(value),
            Err(error) => error_response(&error),
        }
    }
}

/// `{"ok": true, ...}`。
pub fn ok_response(fields: Value) -> Value {
    let mut object = Map::new();
    object.insert("ok".to_owned(), Value::Bool(true));
    if let Value::Object(map) = fields {
        for (key, value) in map {
            object.insert(key, value);
        }
    }
    Value::Object(object)
}

/// 5.7 错误响应。
pub fn error_response(error: &YanshiError) -> Value {
    error.to_response()
}

/// 预览信息来源：新渲染（含尺寸与哈希）或已缓存地址。
#[derive(Debug, Clone, PartialEq)]
pub enum PreviewInfo {
    /// 本次渲染产生的预览。
    Fresh(Box<RenderedPreview>),
    /// 之前渲染留下的地址（避免只读工具重新渲染）。
    Cached(String),
}

impl PreviewInfo {
    /// 10.1 的 `preview` 字段。
    pub fn to_json(&self) -> Value {
        match self {
            Self::Fresh(preview) => json!({
                "thumb_url": preview.url,
                "blob_hash": preview.blob_hash,
                "mime_type": preview.mime_type,
                "width": preview.width,
                "height": preview.height,
            }),
            Self::Cached(url) => json!({"thumb_url": url}),
        }
    }

    /// 取回地址。
    pub fn url(&self) -> &str {
        match self {
            Self::Fresh(preview) => &preview.url,
            Self::Cached(url) => url,
        }
    }
}

/// 10.1 成功返回格式。
pub fn commit_response(
    result: &CommitResult,
    preview: Option<&PreviewInfo>,
    job_status: Option<&str>,
    suggestions: Vec<Value>,
) -> Value {
    json!({
        "ok": true,
        "atom_id": result.atom_id,
        "seq": result.seq,
        "duplicate": result.duplicate,
        "changeset_id": result.changeset_id,
        "head": result.head_seq,
        "dirty_kind": result.dirty_kind,
        "dirty_bbox": result.dirty_bbox,
        "dirty_tiles": result.dirty_tiles.len(),
        "preview": preview.map(PreviewInfo::to_json),
        "job_id": result.job_id,
        "job_status": job_status,
        "warnings": result.warnings,
        "suggestions": suggestions,
        "snapshotted": result.snapshotted,
    })
}

fn input_schema(tool: &ToolSpec) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for param in tool.params {
        let mut schema = json!({"type": param.kind.json_type(), "description": param.description});
        if let Value::Object(map) = &mut schema {
            if param.kind == ParamKind::Object {
                map.insert("additionalProperties".to_owned(), Value::Bool(true));
            }
            if param.kind == ParamKind::Array {
                map.insert("items".to_owned(), json!({}));
            }
        }
        properties.insert(param.name.to_owned(), schema);
        if param.required {
            required.push(Value::String(param.name.to_owned()));
        }
    }
    json!({
        "name": tool.name,
        "description": tool.summary,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        }
    })
}

// ---------------------------------------------------------------------------
// 参数辅助
// ---------------------------------------------------------------------------

const NO_PARAMS: &[ParamSpec] = &[];

fn args_object(args: &Value) -> Result<&Map<String, Value>> {
    args.as_object().ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("工具参数必须是 JSON 对象"),
        )
    })
}

/// 读取必需数组参数（返回数组元素的克隆，避免借用冲突）。
fn require_array(args: &Value, key: &str) -> Result<Vec<Value>> {
    args.get(key)
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("缺少必需参数 {key}（应为数组）")),
            )
        })
}

fn require_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| missing(key))
}

fn optional_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn require_object<'a>(args: &'a Value, key: &str) -> Result<&'a Value> {
    args.get(key)
        .filter(|value| value.is_object() || value.is_array())
        .ok_or_else(|| missing(key))
}

fn optional_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

/// 校验 `data.color` / `data.stroke_color`（统一由 `yanshi_render` 解析）。
///
/// 非法颜色必须在工具层就被拒绝：历史事故里 `[40,120,60,255]` 被渲染层当作线性浮点，
/// alpha=255 饱和成白色，用户「画了一笔却什么都没看见」。
fn validate_colors(data: &Value) -> Result<()> {
    for key in ["color", "stroke_color", "fill_color", "background_color"] {
        if let Some(value) = data.get(key) {
            if value.is_null() {
                continue;
            }
            if let Some(message) = yanshi_render::color::color_error(value) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("data.{key} {message}")),
                ));
            }
        }
    }
    Ok(())
}

fn missing(key: &str) -> YanshiError {
    YanshiError::new(
        ErrorCode::InvalidArgument,
        ErrorContext::detail(format!("缺少必填参数 {key}")),
    )
}

fn parse_bbox(value: &Value) -> Result<Bbox> {
    if let Some(bbox) = Bbox::from_value(value) {
        return Ok(bbox);
    }
    if let Some(array) = value.as_array() {
        if array.len() == 4 {
            let numbers: Vec<f64> = array.iter().filter_map(Value::as_f64).collect();
            if numbers.len() == 4 {
                return Ok(Bbox::new(numbers[0], numbers[1], numbers[2], numbers[3]));
            }
        }
    }
    Err(YanshiError::new(
        ErrorCode::InvalidArgument,
        ErrorContext::detail("region 必须是 {x,y,w,h} 或 [x,y,w,h]"),
    ))
}

fn document_state(ctx: &mut ToolContext<'_>) -> Result<DocumentState> {
    // 只读工具需要 &DocumentState；克隆一份避免把借用带出闭包（状态很小，读取频率低）。
    ctx.workspace
        .document_mut(&ctx.doc_id)
        .map(|document| document.state().clone())
}

/// 提交后的收尾：按 `wait_for_render` 决定是否等待渲染完成，并给出预览。
fn finish_mutation(
    ctx: &mut ToolContext<'_>,
    result: &CommitResult,
    preview_region: Option<Bbox>,
) -> Result<Value> {
    let mut job_status = None;
    if let Some(job_id) = &result.job_id {
        if ctx.wait_for_render {
            // 进程内实现是同步渲染：在预算内直接跑完 job（6.7 的 wait_for_render）。
            let document = ctx.workspace.document_mut(&ctx.doc_id)?;
            let _ = document.run_pending_jobs()?;
            let job = document.jobs_mut().get(job_id, ctx.now)?;
            job_status = Some(job.status.as_str().to_owned());
        } else {
            job_status = Some(JobStatus::Submitted.as_str().to_owned());
        }
    }

    let mut preview = None;
    if ctx.wait_for_render {
        // 区域预览（8.2/8.3）：只渲染 dirty 区域，避免每次修改都全图重算。
        match preview_region {
            Some(bbox) => {
                let document = ctx.workspace.document_mut(&ctx.doc_id)?;
                let (width, height) = (document.state().width, document.state().height);
                let x = bbox.x.max(0.0).min((width as f64 - 1.0).max(0.0));
                let y = bbox.y.max(0.0).min((height as f64 - 1.0).max(0.0));
                let w = bbox.w.max(1.0).min(width as f64 - x);
                let h = bbox.h.max(1.0).min(height as f64 - y);
                let region = Bbox::new(x, y, w.max(1.0), h.max(1.0));
                let rendered = ctx.workspace.render_region(&ctx.doc_id, region)?;
                preview = Some(PreviewInfo::Fresh(Box::new(rendered)));
            }
            None => {
                let document = ctx.workspace.document_mut(&ctx.doc_id)?;
                if let Some(url) = document.latest_preview_url() {
                    preview = Some(PreviewInfo::Cached(url));
                }
            }
        }
    }
    Ok(commit_response(
        result,
        preview.as_ref(),
        job_status.as_deref(),
        Vec::new(),
    ))
}

fn region_of(result: &CommitResult) -> Option<Bbox> {
    result
        .dirty_bbox
        .map(|b| Bbox::new(b[0], b[1], b[2].max(1.0), b[3].max(1.0)))
}

// ---------------------------------------------------------------------------
// 工具表（10.2）
// ---------------------------------------------------------------------------

const ID_ARGS: &[ParamSpec] = &[param!("object_id", String, true, "对象 id")];
const LAYER_ID: &[ParamSpec] = &[param!("layer_id", String, true, "图层 id")];

/// 核心层 27 个 + 扩展组中已实现的工具（10.2）。
pub const ALL_TOOLS: &[ToolSpec] = &[
    // ---- 查询 ----
    ToolSpec {
        name: "get_document",
        profile: Profile::Core,
        summary: "读取文档元信息与统计（尺寸、head、图层/对象数、渲染水位、文档级缩略图）",
        mutating: false,
        params: &[param!(
            "preview_size",
            Any,
            false,
            "缩略图档位 64/128/256 或 false 跳过（缺省 256）"
        )],
    },
    ToolSpec {
        name: "get_state",
        profile: Profile::Core,
        summary: "读取当前 HEAD 状态摘要与预览地址",
        mutating: false,
        params: &[
            param!("include_objects", Boolean, false, "是否包含对象列表"),
            param!("include_hidden", Boolean, false, "是否包含不可见图层/对象"),
            param!(
                "preview_size",
                Any,
                false,
                "缩略图档位 64/128/256 或 false（缺省 256）"
            ),
        ],
    },
    ToolSpec {
        name: "list_layers",
        profile: Profile::Core,
        summary: "列出图层（z 序、可见性、不透明度、混合模式、蒙版）",
        mutating: false,
        params: NO_PARAMS,
    },
    ToolSpec {
        name: "list_objects",
        profile: Profile::Core,
        summary: "列出对象（可按图层与类型过滤）",
        mutating: false,
        params: &[
            param!("layer_id", String, false, "按图层过滤"),
            param!("type", String, false, "按对象类型过滤"),
            param!("include_hidden", Boolean, false, "是否包含不可见对象"),
        ],
    },
    ToolSpec {
        name: "get_object",
        profile: Profile::Core,
        summary: "读取对象详情（含包围盒与版本链）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("include_history", Boolean, false, "是否包含版本链"),
        ],
    },
    ToolSpec {
        name: "render_region",
        profile: Profile::Core,
        summary: "渲染区域并返回预览地址（8.3 局部渲染）",
        mutating: false,
        params: &[
            param!("region", Any, true, "区域 {x,y,w,h} 或 [x,y,w,h]"),
            param!("raw", Boolean, false, "返回未编码的直通 RGBA8（写入 CAS 并返回 raw_url），供逐像素自检"),
            param!(
                "include_image",
                Boolean,
                false,
                "是否内嵌 base64 PNG（≤512px）"
            ),
        ],
    },
    // ---- 图层 ----
    ToolSpec {
        name: "create_layer",
        profile: Profile::Core,
        summary: "创建图层",
        mutating: true,
        params: &[
            param!("layer_id", String, false, "图层 id（缺省自动生成）"),
            param!("name", String, false, "图层名"),
            param!(
                "type",
                String,
                false,
                "raster | vector | layer_group | mask"
            ),
            param!("parent_id", String, false, "父图层（图层组）"),
            param!("z_index", Integer, false, "z 序"),
        ],
    },
    ToolSpec {
        name: "update_layer",
        profile: Profile::Core,
        summary: "修改图层属性（名称、可见性、不透明度、混合模式、锁定、蒙版）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "图层 id"),
            param!(
                "patch",
                Object,
                true,
                "属性补丁，如 {visible:false, opacity:0.5}"
            ),
        ],
    },
    ToolSpec {
        name: "delete_layer",
        profile: Profile::Core,
        summary: "删除图层（tombstone，级联到后代与其中对象）",
        mutating: true,
        params: LAYER_ID,
    },
    ToolSpec {
        name: "reorder_layers",
        profile: Profile::Core,
        summary: "重排图层（必须携带完整目标 z 序）",
        mutating: true,
        params: &[param!("order", Array, true, "完整图层 id 顺序（自底向上）")],
    },
    // ---- 导入 ----
    ToolSpec {
        name: "import_image",
        profile: Profile::Core,
        summary: "导入位图（blob 先行，创建像素图层对象）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("bitmap", Object, true, "{blob_hash,size,mime_type}"),
            param!("region", Object, true, "放置区域 {x,y,w,h}"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    // ---- 绘制 ----
    ToolSpec {
        name: "draw_stroke",
        profile: Profile::Core,
        summary: "绘制笔触（data.color 支持 [r,g,b,a] 0-1 线性 / 0-255 字节、{r,g,b,a}、#RRGGBB）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!(
                "data",
                Object,
                true,
                "{points,size,color,hardness,opacity,seed...}"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "draw_shape",
        profile: Profile::Core,
        summary: "绘制形状（矩形/椭圆/多边形，可描边）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!(
                "data",
                Object,
                true,
                "{geometry,color,stroke_width,stroke_color}"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "draw_text",
        profile: Profile::Core,
        summary: "绘制文本对象（内核文本光栅化待字体子集，属路线图）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!(
                "data",
                Object,
                true,
                "{text,font,size,color,position,align}"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "fill",
        profile: Profile::Core,
        summary: "填充区域或对象",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("data", Object, true, "{color,region|object_id}"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "erase",
        profile: Profile::Core,
        summary: "擦除（对象新版本或区域擦除）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("data", Object, true, "{region,hardness,opacity}"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    // ---- 修改 ----
    ToolSpec {
        name: "update_object",
        profile: Profile::Core,
        summary: "修改对象属性（可见性、锁定、z 序、图层归属、元数据）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("patch", Object, true, "属性补丁"),
        ],
    },
    ToolSpec {
        name: "update_stroke",
        profile: Profile::Core,
        summary: "修改笔触（10.3 三层参数：核心 / preset / advanced）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!(
                "core",
                Object,
                false,
                "核心参数 {points|points_patch,brush,color,size,opacity,blend_mode}"
            ),
            param!("preset", String, false, "命名预设"),
            param!(
                "advanced",
                Object,
                false,
                "geometry/appearance/structure/metadata"
            ),
        ],
    },
    ToolSpec {
        name: "move_object",
        profile: Profile::Core,
        summary: "移动/变换对象",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("delta", Object, false, "{dx,dy} 平移"),
            param!("transform", Object, false, "{matrix,pivot} 仿射变换"),
        ],
    },
    // ---- 删除 ----
    ToolSpec {
        name: "delete_object",
        profile: Profile::Core,
        summary: "删除对象（tombstone）",
        mutating: true,
        params: ID_ARGS,
    },
    // ---- 撤销 ----
    ToolSpec {
        name: "revert",
        profile: Profile::Core,
        summary: "撤销原子（不删除原原子；级联失效由折叠报告）",
        mutating: true,
        params: &[param!("atom_id", String, true, "目标原子 id")],
    },
    ToolSpec {
        name: "reapply",
        profile: Profile::Core,
        summary: "恢复被撤销的原子（不恢复级联链）",
        mutating: true,
        params: &[param!("atom_id", String, true, "目标原子 id")],
    },
    // ---- 历史 ----
    ToolSpec {
        name: "get_log",
        profile: Profile::Core,
        summary: "读取原子日志元数据（MCP 轮询通道）",
        mutating: false,
        params: &[
            param!("since_seq", Integer, false, "只返回 seq 大于它的原子"),
            param!("limit", Integer, false, "最多返回条数"),
            param!("kind", String, false, "按原子类型过滤"),
            param!("actor", String, false, "按操作者过滤"),
        ],
    },
    // ---- Job ----
    ToolSpec {
        name: "get_job",
        profile: Profile::Core,
        summary: "查询 Job 状态机与结果",
        mutating: false,
        params: &[param!("job_id", String, true, "job id")],
    },
    ToolSpec {
        name: "get_render_status",
        profile: Profile::Core,
        summary: "查询某个原子是否已渲染",
        mutating: false,
        params: &[param!("atom_id", String, true, "原子 id")],
    },
    ToolSpec {
        name: "cancel_job",
        profile: Profile::Core,
        summary: "取消运行中的 Job",
        mutating: false,
        params: &[param!("job_id", String, true, "job id")],
    },
    // ---- 批量 ----
    ToolSpec {
        name: "batch",
        profile: Profile::Core,
        summary: "批量调用（同一变更集，可整体撤销）",
        mutating: true,
        params: &[
            param!("calls", Array, true, "[{tool, arguments}]"),
            param!("message", String, false, "变更集说明"),
        ],
    },
    // ---- 扩展：history ----
    ToolSpec {
        name: "checkpoint",
        profile: Profile::History,
        summary: "创建检查点（用户语义，锚定 anchor_seq）",
        mutating: true,
        params: &[
            param!("name", String, false, "检查点名"),
            param!("message", String, false, "说明"),
        ],
    },
    ToolSpec {
        name: "get_checkpoints",
        profile: Profile::History,
        summary: "列出检查点",
        mutating: false,
        params: NO_PARAMS,
    },
    ToolSpec {
        name: "declare_head",
        profile: Profile::History,
        summary: "求值起点跳变（declare_head，5.5）",
        mutating: true,
        params: &[
            param!("base_type", String, true, "atom | checkpoint"),
            param!("base_id", String, true, "目标原子或检查点 id"),
            param!("reason", String, false, "原因"),
        ],
    },
    ToolSpec {
        name: "revert_to",
        profile: Profile::History,
        summary: "回到某个原子时刻（通过 declare_head 实现）",
        mutating: true,
        params: &[param!("atom_id", String, true, "目标原子 id")],
    },
    ToolSpec {
        name: "restore_checkpoint",
        profile: Profile::History,
        summary: "恢复到检查点（通过 declare_head 实现）",
        mutating: true,
        params: &[param!("checkpoint_id", String, true, "检查点 id")],
    },
    // ---- 扩展：annotation ----
    ToolSpec {
        name: "create_annotation",
        profile: Profile::Annotation,
        summary: "创建标注（独立 append-only 通道，不进原子日志）",
        mutating: false,
        params: &[
            param!(
                "type",
                String,
                true,
                "region|object|arrow|text|doodle|highlight"
            ),
            param!(
                "intent",
                String,
                true,
                "modify|add|remove|replace|style|move|resize|color"
            ),
            param!(
                "target",
                Object,
                true,
                "{target:'region',bbox} 或 {target:'object',object_id}"
            ),
            param!("content", String, false, "内容或路径"),
            param!("suggestion_id", String, false, "关联建议 id（4b：标注 → 建议的追踪）"),
        ],
    },
    ToolSpec {
        name: "update_annotation",
        profile: Profile::Annotation,
        summary: "更新标注（追加新版本）；status=pending 表示重开（已 resolved/rejected 的标注须先重开才能再解决）",
        mutating: false,
        params: &[
            param!("annotation_id", String, true, "标注 id"),
            param!("content", String, false, "新内容"),
            param!("intent", String, false, "新意图"),
            param!("status", String, false, "仅支持 pending：把已解决/已拒绝的标注重开为待处理"),
        ],
    },
    ToolSpec {
        name: "delete_annotation",
        profile: Profile::Annotation,
        summary: "删除标注（标记为 rejected，保留历史）",
        mutating: false,
        params: &[param!("annotation_id", String, true, "标注 id")],
    },
    ToolSpec {
        name: "list_annotations",
        profile: Profile::Annotation,
        summary: "列出标注（按状态/作者/意图/对象过滤）",
        mutating: false,
        params: &[
            param!("status", String, false, "pending|resolved|rejected"),
            param!("actor", String, false, "按作者过滤"),
            param!("intent", String, false, "按意图过滤"),
            param!("object_id", String, false, "按对象目标过滤"),
            param!("suggestion_id", String, false, "按关联建议过滤（4b：查「引用了该建议的标注」）"),
        ],
    },
    ToolSpec {
        name: "get_annotation",
        profile: Profile::Annotation,
        summary: "读取标注当前版本与修订历史",
        mutating: false,
        params: &[param!("annotation_id", String, true, "标注 id")],
    },
    ToolSpec {
        name: "resolve_annotation",
        profile: Profile::Annotation,
        summary: "标记标注已解决（可关联解决它的原子）",
        mutating: false,
        params: &[
            param!("annotation_id", String, true, "标注 id"),
            param!("atom_id", String, false, "解决它的原子"),
        ],
    },
    ToolSpec {
        name: "reject_annotation",
        profile: Profile::Annotation,
        summary: "拒绝标注",
        mutating: false,
        params: &[param!("annotation_id", String, true, "标注 id")],
    },
    // ---- 扩展：structure（蒙版；内核已实现按形状覆盖率 + 羽化 + 反选调制图层 alpha）----
    ToolSpec {
        name: "create_mask",
        profile: Profile::Structure,
        summary: "创建蒙版（形状覆盖率 + 羽化 + 反选），随后用 set_property 把 mask_id 挂到图层",
        mutating: true,
        params: &[
            param!("mask_id", String, true, "蒙版 id"),
            param!("shape", Object, true, "几何 {kind:rect|ellipse|polygon, bbox, points?}"),
            param!("feather", Number, false, "羽化过渡宽度（像素，缺省 0）"),
            param!("invert", Boolean, false, "反选（缺省 false）"),
            param!("mode", String, false, "组合模式 new/add/subtract/intersect（缺省 new）"),
            param!("linked_layer", String, false, "关联图层 id"),
        ],
    },
    // ---- 扩展：retouch（调色与滤镜；内核已实现的子集）----
    ToolSpec {
        name: "add_adjustment",
        profile: Profile::Retouch,
        summary: "新增调整图层对象（调色）：brightness_contrast / saturation / vibrance / invert / levels（可分通道）/ exposure / white_balance / curves / hsl / posterize / color_balance / split_toning",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("adjustment_type", String, true, "调整类型（见工具说明）"),
            param!(
                "params",
                Object,
                false,
                "类型参数，如 {brightness:0.1, contrast:1.2}"
            ),
            param!("opacity", Number, false, "整体不透明度 0-1（缺省 1）"),
            param!(
                "z_index",
                Integer,
                false,
                "层内 z 序（缺省放到同层最上方，作用于全部下方内容）"
            ),
            param!("name", String, false, "对象名（元数据）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "add_filter",
        profile: Profile::Retouch,
        summary:
            "新增滤镜图层对象：box_blur / gaussian_blur / brightness_contrast / saturation / invert",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("filter_name", String, true, "滤镜名（见工具说明）"),
            param!("params", Object, false, "滤镜参数，如 {sigma:2.0}"),
            param!("opacity", Number, false, "整体不透明度 0-1（缺省 1）"),
            param!(
                "z_index",
                Integer,
                false,
                "层内 z 序（缺省放到同层最上方，作用于全部下方内容）"
            ),
            param!("name", String, false, "对象名（元数据）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "update_adjustment",
        profile: Profile::Retouch,
        summary: "修改调整图层参数（叠加到现有参数上）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "调整对象 id"),
            param!("params", Object, true, "要覆盖的参数"),
            param!("opacity", Number, false, "整体不透明度 0-1"),
        ],
    },
    ToolSpec {
        name: "update_filter",
        profile: Profile::Retouch,
        summary: "修改滤镜参数（叠加到现有参数上）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "滤镜对象 id"),
            param!("params", Object, true, "要覆盖的参数"),
            param!("opacity", Number, false, "整体不透明度 0-1"),
        ],
    },
    ToolSpec {
        name: "list_effects",
        profile: Profile::Retouch,
        summary: "列出当前文档的调整/滤镜对象（含参数与生效顺序）",
        mutating: false,
        params: &[param!("layer_id", String, false, "按图层过滤")],
    },
    // ---- 扩展：retouch（修图）----
    ToolSpec {
        name: "estimate_dehaze",
        profile: Profile::Retouch,
        summary: "估计去雾参数（只读）：扫描当前画面给出建议大气光 air 与暗通道均值，供 add_filter(dehaze) 使用",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "liquify_push",
        profile: Profile::Retouch,
        summary: "基础液化（推力）：把笔迹范围内的像素沿 direction 推开（反向映射 + 双线性重采样）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("direction", Array, true, "推力方向 [dx,dy]（非零）"),
            param!("size", Number, false, "影响直径（缺省 80）"),
            param!("strength", Number, false, "强度 0-2（缺省 0.5）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "liquify_twirl",
        profile: Profile::Retouch,
        summary: "液化（旋转）：把笔迹范围内的像素绕笔迹点旋转（强度为弧度上限，负值反向）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("size", Number, false, "影响直径（缺省 80）"),
            param!("strength", Number, false, "强度 0-2（缺省 0.5，1.0 ≈ 57°）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "liquify_pinch",
        profile: Profile::Retouch,
        summary: "液化（收缩/膨胀）：正强度把内容吸向中心，负强度向外膨胀",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("size", Number, false, "影响直径（缺省 80）"),
            param!("strength", Number, false, "强度 -2..2（缺省 0.5；正=收缩）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "patch",
        profile: Profile::Retouch,
        summary: "图章补丁：把 source_region 的像素抓取为 blob，并作为 raster_patch 落到 target 位置（blob 先行，6.3）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("source_region", Object, true, "源区域 {x,y,w,h}"),
            param!("target", Array, true, "落点左上角 [x,y]"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "smudge",
        profile: Profile::Retouch,
        summary: "涂抹：沿笔迹方向把后方的已有内容拖到前方（每 stamp 后退 smudge_length 像素采样）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("size", Number, false, "笔刷直径（缺省 24）"),
            param!("smudge_length", Number, false, "采样后退距离（缺省 12）"),
            param!("hardness", Number, false, "硬度 0-1（缺省 0.6）"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "heal_stamp",
        profile: Profile::Retouch,
        summary: "修复画笔：复制 source_offset 处的纹理，并把低频颜色/明度对齐到目标处",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("source_offset", Array, true, "采样偏移 [dx,dy]，源 = 目标 + 偏移"),
            param!("size", Number, false, "笔刷直径（缺省 24）"),
            param!("hardness", Number, false, "硬度 0-1（缺省 0.6）"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    ToolSpec {
        name: "clone_stamp",
        profile: Profile::Retouch,
        summary: "仿制图章：把 source_offset 处的已有内容复制到 points 轨迹上（源为应用本对象前的图层内容）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("points", Array, true, "笔迹点列 [[x,y], ...]"),
            param!("source_offset", Array, true, "采样偏移 [dx,dy]，源 = 目标 + 偏移"),
            param!("size", Number, false, "笔刷直径（缺省 24）"),
            param!("hardness", Number, false, "硬度 0-1（缺省 0.6）"),
            param!("opacity", Number, false, "不透明度 0-1（缺省 1）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
            param!("z_index", Integer, false, "层内 z 序（缺省放到同层最上方）"),
        ],
    },
    // ---- 扩展：collab ----
    ToolSpec {
        name: "suggest",
        profile: Profile::Collab,
        summary: "记录一条建议（suggest 原子，含 patch）：AI 解析标注后提出可执行的补丁步骤",
        mutating: true,
        params: &[
            param!("patch", Array, true, "补丁步骤 [{\"tool\":\"add_filter\",\"arguments\":{...}}, ...]（非空）"),
            param!("annotation_id", String, false, "关联的标注 id（写入建议以追踪来源）"),
            param!("summary", String, false, "人类可读的说明"),
            param!("priority", Integer, false, "优先级 0..9（越大越紧急，缺省 5）；list_suggestions 按它排序"),
        ],
    },
    ToolSpec {
        name: "accept_suggestions",
        profile: Profile::Collab,
        summary: "批量接受建议（最多 64 条）：逐条重放 patch，个别失败不中断，逐条返回结果",
        mutating: true,
        params: &[
            param!("suggestion_ids", Array, true, "建议 id 列表"),
        ],
    },
    ToolSpec {
        name: "preview_suggestion",
        profile: Profile::Collab,
        summary: "预览建议（不应用）：逐步校验工具名与参数、报告目标与变更类别，便于人工/AI 审阅",
        mutating: false,
        params: &[
            param!("suggestion_id", String, false, "建议 id（与 patch 二选一）"),
            param!("patch", Array, false, "直接给出 patch 做预校验（与 suggestion_id 二选一）"),
        ],
    },
    ToolSpec {
        name: "accept_suggestion",
        profile: Profile::Collab,
        summary: "接受建议：按序重放 patch（只允许会产生状态效果的步骤），提交 accept_suggestion 原子并把关联标注置为 resolved",
        mutating: true,
        params: &[
            param!("suggestion_id", String, true, "建议 id（suggest 原子的 id）"),
        ],
    },
    ToolSpec {
        name: "reject_suggestion",
        profile: Profile::Collab,
        summary: "拒绝建议：记录原因，并把引用该建议的标注置为 rejected（12.6）",
        mutating: true,
        params: &[
            param!("suggestion_id", String, true, "建议 id（suggest 原子的 id）"),
            param!("reason", String, false, "拒绝原因"),
        ],
    },
    ToolSpec {
        name: "reject_suggestions",
        profile: Profile::Collab,
        summary: "批量拒绝建议（最多 64 条）：逐条记录原因并联动标注状态，个别失败不中断",
        mutating: true,
        params: &[
            param!("suggestion_ids", Array, true, "建议 id 列表"),
            param!("reason", String, false, "统一的拒绝原因"),
        ],
    },
    ToolSpec {
        name: "list_suggestions",
        profile: Profile::Collab,
        summary: "列出建议及其状态（pending / accepted / rejected，由 accept/reject 原子推导）",
        mutating: false,
        params: &[
            param!("status", String, false, "按状态过滤：pending/accepted/rejected"),
            param!("since_seq", Integer, false, "只返回 seq 大于该值的建议（AI 轮询增量用）"),
            param!("limit", Integer, false, "最多返回多少条（缺省 50，上限 500）"),
            param!("offset", Integer, false, "跳过前 N 条（分页）"),
        ],
    },
    ToolSpec {
        name: "comment",
        profile: Profile::Collab,
        summary: "发表评论（协作原子，不产生状态效果）",
        mutating: true,
        params: &[
            param!("text", String, true, "评论内容"),
            param!("target_atom", String, false, "被评论的原子"),
            param!("object_id", String, false, "被评论的对象"),
        ],
    },
    // ---- 扩展：structure ----
    ToolSpec {
        name: "set_property",
        profile: Profile::Structure,
        summary: "设置对象/图层属性（低层原子 set_property）",
        mutating: true,
        params: &[
            param!("key", String, true, "属性名"),
            param!("value", Any, false, "属性值（类型随 key 变化：布尔/字符串/对象）"),
            param!("object_id", String, false, "对象 id"),
            param!("layer_id", String, false, "图层 id"),
        ],
    },
    ToolSpec {
        name: "lock_layer",
        profile: Profile::Structure,
        summary: "锁定图层",
        mutating: true,
        params: LAYER_ID,
    },
    ToolSpec {
        name: "unlock_layer",
        profile: Profile::Structure,
        summary: "解锁图层",
        mutating: true,
        params: LAYER_ID,
    },
    ToolSpec {
        name: "list_brushes",
        profile: Profile::Structure,
        summary: "列出内置通用光栅笔刷参数（11.1 MVP 笔刷）",
        mutating: false,
        params: NO_PARAMS,
    },
];

// ---------------------------------------------------------------------------
// 分发
// ---------------------------------------------------------------------------

fn dispatch(spec: &ToolSpec, ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    match spec.name {
        "get_document" => read_get_document(ctx, args),
        "get_state" => read_get_state(ctx, args),
        "list_layers" => read_list_layers(ctx),
        "list_objects" => read_list_objects(ctx, args),
        "get_object" => read_get_object(ctx, args),
        "render_region" => read_render_region(ctx, args),
        "create_layer" => write_create_layer(ctx, args),
        "update_layer" => write_update_layer(ctx, args),
        "delete_layer" => write_delete_layer(ctx, args),
        "reorder_layers" => write_reorder_layers(ctx, args),
        "import_image" => write_import_image(ctx, args),
        "draw_stroke" => write_draw(ctx, args, AtomKind::DrawStroke),
        "draw_shape" => write_draw(ctx, args, AtomKind::DrawShape),
        "draw_text" => write_draw(ctx, args, AtomKind::DrawText),
        "fill" => write_draw(ctx, args, AtomKind::Fill),
        "erase" => write_draw(ctx, args, AtomKind::Erase),
        "update_object" => write_update_object(ctx, args),
        "update_stroke" => write_update_stroke(ctx, args),
        "move_object" => write_move_object(ctx, args),
        "delete_object" => write_tombstone(ctx, args, "object_id"),
        "revert" => write_history_atom(ctx, args, AtomKind::Revert, "atom_id"),
        "reapply" => write_history_atom(ctx, args, AtomKind::Reapply, "atom_id"),
        "get_log" => read_get_log(ctx, args),
        "get_job" => read_get_job(ctx, args),
        "get_render_status" => read_get_render_status(ctx, args),
        "cancel_job" => write_cancel_job(ctx, args),
        "batch" => write_batch(ctx, args),
        "checkpoint" => write_checkpoint(ctx, args),
        "get_checkpoints" => read_get_checkpoints(ctx),
        "declare_head" => write_declare_head(ctx, args),
        "revert_to" => write_revert_to(ctx, args),
        "restore_checkpoint" => write_restore_checkpoint(ctx, args),
        "create_annotation" => write_create_annotation(ctx, args),
        "update_annotation" => write_update_annotation(ctx, args),
        "delete_annotation" => write_delete_annotation(ctx, args),
        "list_annotations" => read_list_annotations(ctx, args),
        "get_annotation" => read_get_annotation(ctx, args),
        "resolve_annotation" => write_resolve_annotation(ctx, args, AnnotationStatus::Resolved),
        "reject_annotation" => write_resolve_annotation(ctx, args, AnnotationStatus::Rejected),
        "add_adjustment" => write_add_effect(ctx, args, EffectKind::Adjustment),
        "add_filter" => write_add_effect(ctx, args, EffectKind::Filter),
        "update_adjustment" => write_update_effect(ctx, args, EffectKind::Adjustment),
        "update_filter" => write_update_effect(ctx, args, EffectKind::Filter),
        "list_effects" => read_list_effects(ctx, args),
        "create_mask" => write_create_mask(ctx, args),
        "clone_stamp" => write_retouch(ctx, args, "clone_stamp"),
        "heal_stamp" => write_retouch(ctx, args, "heal"),
        "smudge" => write_retouch(ctx, args, "smudge"),
        "patch" => write_patch(ctx, args),
        "estimate_dehaze" => read_estimate_dehaze(ctx),
        "liquify_push" => write_liquify(ctx, args, "push"),
        "liquify_twirl" => write_liquify(ctx, args, "twirl"),
        "liquify_pinch" => write_liquify(ctx, args, "pinch"),
        "comment" => write_comment(ctx, args),
        "suggest" => write_suggest(ctx, args),
        "preview_suggestion" => read_preview_suggestion(ctx, args),
        "accept_suggestion" => write_accept_suggestion(ctx, args),
        "accept_suggestions" => write_accept_suggestions(ctx, args),
        "reject_suggestion" => write_reject_suggestion(ctx, args),
        "reject_suggestions" => write_reject_suggestions(ctx, args),
        "list_suggestions" => read_list_suggestions(ctx, args),
        "set_property" => write_set_property(ctx, args),
        "lock_layer" => write_lock_layer(ctx, args, true),
        "unlock_layer" => write_lock_layer(ctx, args, false),
        "list_brushes" => read_list_brushes(),
        other => Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("工具 {other} 尚未实现")),
        )),
    }
}

// ---------------------------------------------------------------------------
// 只读工具
// ---------------------------------------------------------------------------

fn read_get_document(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let mut summary = ctx.workspace.summary_json(&ctx.doc_id)?;
    // 6.2「打开即图片」：返回**文档级**缩略图（覆盖整幅画布），
    // 没有缓存时按 preview_size 现场生成（默认 256；`preview_size: false` 可跳过）。
    let size = args
        .get("preview_size")
        .map(DocThumbSize::parse)
        .unwrap_or(DocThumbSize::S256);
    if let Some(url) = ctx.workspace.ensure_document_thumbnail(&ctx.doc_id, size)? {
        summary["thumb_url"] = json!(url);
        summary["thumb_size"] = json!(size.kind().size());
    }
    summary["preview_size"] = json!(size.kind().size());
    Ok(summary)
}

fn read_get_state(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let include_objects = optional_bool(args, "include_objects").unwrap_or(false);
    let include_hidden = optional_bool(args, "include_hidden").unwrap_or(false);
    let state = document_state(ctx)?;
    let layers: Vec<Value> = state
        .alive_layers()
        .into_iter()
        .filter(|layer| include_hidden || layer.visible)
        .map(|layer| {
            json!({
                "layer_id": layer.id,
                "name": layer.name,
                "type": layer.layer_type,
                "z_index": layer.z_index,
                "visible": layer.visible,
                "opacity": layer.opacity,
                "blend_mode": layer.blend_mode,
                "locked": layer.locked,
                "mask_id": layer.mask_id,
            })
        })
        .collect();
    let mut value = json!({
        "doc_id": ctx.doc_id,
        "head_seq": state.head_seq,
        "width": state.width,
        "height": state.height,
        "color_space": state.color_space,
        "layers": layers,
        "checkpoints": state.checkpoints.len(),
    });
    if include_objects {
        value["objects"] = json!(state
            .alive_objects()
            .into_iter()
            .filter(|object| include_hidden || object.visible)
            .map(summary_object)
            .collect::<Vec<_>>());
    }
    let size = args
        .get("preview_size")
        .map(DocThumbSize::parse)
        .unwrap_or(DocThumbSize::S256);
    if let Some(url) = ctx.workspace.ensure_document_thumbnail(&ctx.doc_id, size)? {
        value["thumb_url"] = json!(url);
    }
    Ok(value)
}

fn summary_object(object: &yanshi_core::Object) -> Value {
    json!({
        "object_id": object.id,
        "layer_id": object.layer_id,
        "type": object.object_type,
        "z_index": object.z_index,
        "visible": object.visible,
        "locked": object.locked,
        "current_version": object.current_version,
        "versions": object.versions.len(),
        "bbox": yanshi_render::object_bbox(object).map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
    })
}

fn read_list_layers(ctx: &mut ToolContext<'_>) -> Result<Value> {
    let state = document_state(ctx)?;
    Ok(json!({
        "layers": state.alive_layers().into_iter().map(|layer| json!({
            "layer_id": layer.id,
            "name": layer.name,
            "type": layer.layer_type,
            "parent_id": layer.parent_id,
            "z_index": layer.z_index,
            "visible": layer.visible,
            "opacity": layer.opacity,
            "blend_mode": layer.blend_mode,
            "locked": layer.locked,
            "alpha_lock": layer.alpha_lock,
            "clipping_mask": layer.clipping_mask,
            "mask_id": layer.mask_id,
            "objects": state.objects_in_layer(&layer.id).len(),
        })).collect::<Vec<_>>(),
        "count": state.alive_layers().len(),
    }))
}

fn read_list_objects(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_filter = optional_str(args, "layer_id");
    let type_filter = optional_str(args, "type");
    let include_hidden = optional_bool(args, "include_hidden").unwrap_or(false);
    let state = document_state(ctx)?;
    let objects: Vec<Value> = state
        .alive_objects()
        .into_iter()
        .filter(|object| {
            layer_filter
                .as_deref()
                .is_none_or(|id| object.layer_id == id)
        })
        .filter(|object| {
            type_filter
                .as_deref()
                .is_none_or(|kind| object_type_name(object.object_type) == kind)
        })
        .filter(|object| include_hidden || object.visible)
        .map(summary_object)
        .collect();
    Ok(json!({"objects": objects, "count": objects.len()}))
}

fn read_get_object(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let include_history = optional_bool(args, "include_history").unwrap_or(false);
    let state = document_state(ctx)?;
    let object = state.objects.get(&object_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        )
        .with_object(object_id.clone())
    })?;
    let mut value = summary_object(object);
    value["data"] = object.data.clone();
    value["transform"] =
        json!({"matrix": object.transform.matrix, "pivot": object.transform.pivot});
    value["created_by"] = json!(object.created_by);
    value["deleted_by"] = json!(object.deleted_by);
    value["blobs"] = json!(object
        .blobs
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>());
    if include_history {
        value["version_chain"] = json!(object.versions);
    }
    Ok(value)
}

fn read_render_region(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // `raw: true`：额外把**原始 RGBA8** 存入 CAS 并返回取回地址，供客户端做逐像素比对
    // （哈希相等无法说明差多少；跨客户端/服务端路径的差异属于 D1 的 ±1 LSB）。
    if args.get("raw").and_then(Value::as_bool).unwrap_or(false) {
        let region = parse_bbox(require_object(args, "region")?)?;
        let (width, height, pixels) = ctx.workspace.render_region_raw(&ctx.doc_id, region)?;
        let raw_hash = ctx.workspace.store().put(&pixels)?;
        let document = ctx.workspace.document_mut(&ctx.doc_id)?;
        return Ok(json!({
            "ok": true,
            "head_seq": document.head_seq(),
            "width": width,
            "height": height,
            "raw_url": format!("yanshi://blob/{raw_hash}"),
            "mime_type": yanshi_render::RAW_RGBA_MIME,
        }));
    }
    let region = parse_bbox(require_object(args, "region")?)?;
    let include_image = optional_bool(args, "include_image").unwrap_or(false);
    let preview = ctx.workspace.render_region(&ctx.doc_id, region)?;
    let mut value = json!({
        "region": preview.bbox,
        "width": preview.width,
        "height": preview.height,
        "thumb_url": preview.url,
        "blob_hash": preview.blob_hash,
        "mime_type": preview.mime_type,
        "bytes": preview.bytes,
        "tiles": preview.tiles,
        "filter_padding": preview.filter_padding,
        "warnings": preview.warnings,
    });
    // 7.5：MCP 响应可选内嵌小尺寸 image content（≤512px），其余走 URL。
    if include_image && preview.width <= 512 && preview.height <= 512 {
        let png = ctx.workspace.store().get(&preview.blob_hash)?;
        value["image"] = json!({
            "mime_type": preview.mime_type,
            "data": base64::encode(&png),
            "width": preview.width,
            "height": preview.height,
        });
    }
    Ok(value)
}

fn read_get_log(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let since = optional_u64(args, "since_seq").unwrap_or(0);
    let limit = optional_u64(args, "limit").unwrap_or(200).min(2000) as usize;
    let kind_filter = optional_str(args, "kind");
    let actor_filter = optional_str(args, "actor");
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let atoms: Vec<Value> = document
        .log()
        .iter()
        .filter(|atom| atom.seq > since)
        .filter(|atom| {
            kind_filter
                .as_deref()
                .is_none_or(|kind| atom.kind.as_str() == kind)
        })
        .filter(|atom| {
            actor_filter
                .as_deref()
                .is_none_or(|actor| atom.actor == actor)
        })
        .take(limit)
        .map(|atom| {
            json!({
                "atom_id": atom.id,
                "seq": atom.seq,
                "kind": atom.kind,
                "actor": atom.actor,
                "session": atom.session,
                "changeset_id": atom.changeset_id,
                "timestamp": atom.timestamp,
                "heavy": atom.is_heavy(),
                "state_effect": atom.kind.is_state_effect(),
            })
        })
        .collect();
    Ok(json!({
        "atoms": atoms,
        "count": atoms.len(),
        "head_seq": document.head_seq(),
        "next_since": atoms.last().map(|atom| atom["seq"].clone()).unwrap_or(json!(since)),
    }))
}

fn read_get_job(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let job_id = require_str(args, "job_id")?;
    let document = ctx.workspace.document_mut(&ctx.doc_id)?;
    let job = document.jobs_mut().get(&job_id, ctx.now)?;
    Ok(json!({
        "job_id": job.id,
        "kind": job.kind,
        "session": job.session,
        "status": job.status,
        "atom_id": job.atom_id,
        "error": job.error,
        "created_at": job.created_at,
        "ttl": job.ttl,
        "terminal": job.status.is_terminal(),
    }))
}

fn read_get_render_status(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let atom_id = require_str(args, "atom_id")?;
    let status = ctx.workspace.render_status(&ctx.doc_id, &atom_id)?;
    Ok(json!({
        "rendered": status.rendered,
        "head_seq": status.head_seq,
        "rendered_seq": status.rendered_seq,
        "atom_seq": status.atom_seq,
        "thumb_url": status.thumb_url,
    }))
}

fn read_get_checkpoints(ctx: &mut ToolContext<'_>) -> Result<Value> {
    let state = document_state(ctx)?;
    let checkpoints: Vec<Value> = state
        .checkpoints
        .values()
        .map(|checkpoint| {
            json!({
                "checkpoint_id": checkpoint.id,
                "name": checkpoint.name,
                "created_by": checkpoint.created_by,
                "message": checkpoint.message,
                "anchor_seq": checkpoint.anchor_seq,
                "snapshot_id": checkpoint.snapshot_id,
            })
        })
        .collect();
    Ok(json!({"checkpoints": checkpoints, "count": checkpoints.len()}))
}

fn read_list_brushes() -> Result<Value> {
    // MVP 只做通用光栅笔刷（11.1）；其他介质通过 WASM 插件扩展。
    Ok(json!({
        "brushes": [
            {"id": "round", "name": "圆头笔刷", "params": {"size": 1.0, "hardness": 0.8, "flow": 1.0, "opacity": 1.0, "spacing": 0.25}},
            {"id": "soft_round", "name": "软圆笔刷", "params": {"size": 24.0, "hardness": 0.0, "flow": 0.6, "opacity": 1.0, "spacing": 0.15}},
            {"id": "jitter", "name": "抖动笔刷", "params": {"size": 6.0, "hardness": 0.6, "jitter": 2.0, "seed": 0}},
            {"id": "dashed", "name": "虚线笔刷", "params": {"size": 3.0, "dash": 6.0}},
        ],
        "note": "其他介质（油画/水彩/马克笔/铅笔/像素/矢量）通过 WASM 插件扩展，属路线图",
    }))
}

// ---------------------------------------------------------------------------
// 写入工具
// ---------------------------------------------------------------------------

fn write_create_layer(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = optional_str(args, "layer_id")
        .unwrap_or_else(|| format!("layer_{}", yanshi_core::Ulid::new().encode()));
    let mut payload = json!({
        "layer_id": layer_id,
        "name": optional_str(args, "name").unwrap_or_else(|| layer_id.clone()),
    });
    for key in ["type", "parent_id"] {
        if let Some(value) = optional_str(args, key) {
            payload[key] = json!(value);
        }
    }
    if let Some(z_index) = args.get("z_index").and_then(Value::as_i64) {
        payload["z_index"] = json!(z_index);
    }
    let result = ctx.commit(AtomKind::CreateLayer, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

const LAYER_PATCH_KEYS: [&str; 8] = [
    "name",
    "visible",
    "opacity",
    "blend_mode",
    "locked",
    "alpha_lock",
    "clipping_mask",
    "mask_id",
];

fn write_update_layer(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let patch = require_object(args, "patch")?.clone();
    let patch = patch.as_object().ok_or_else(|| missing("patch"))?;
    let mut atoms = Vec::new();
    for key in LAYER_PATCH_KEYS {
        if let Some(value) = patch.get(key) {
            atoms.push(Atom::new(
                AtomKind::SetProperty,
                ctx.actor.clone(),
                ctx.session.clone(),
                json!({"layer_id": layer_id, "key": key, "value": value}),
            ));
        }
    }
    if atoms.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("patch 中没有可修改的图层属性"),
        ));
    }
    let changeset = Changeset::new_id();
    let results = ctx.commit_changeset(atoms, changeset.clone())?;
    let responses: Vec<Value> = results
        .iter()
        .map(|result| commit_response(result, None, None, Vec::new()))
        .collect();
    Ok(json!({
        "changeset_id": changeset,
        "atoms": responses,
        "count": results.len(),
        "head": results.last().map(|result| result.head_seq),
    }))
}

fn write_delete_layer(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let result = ctx.commit(AtomKind::Tombstone, json!({"layer_id": layer_id}))?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_reorder_layers(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let order = require_object(args, "order")?;
    let order = order.as_array().ok_or_else(|| missing("order"))?;
    let ids: Vec<String> = order
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if ids.len() != order.len() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("order 必须全部是图层 id 字符串"),
        ));
    }
    // 工具层先校验完整性，给出比折叠级联失效更友好的错误（5.3 要求完整目标 z 序）。
    let state = document_state(ctx)?;
    let alive: BTreeSet<String> = state
        .alive_layers()
        .iter()
        .map(|layer| layer.id.clone())
        .collect();
    let provided: BTreeSet<String> = ids.iter().cloned().collect();
    if alive != provided {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "order 必须是全部存活图层的完整顺序（期望 {} 个，实际 {} 个）",
                alive.len(),
                provided.len()
            )),
        ));
    }
    let result = ctx.commit(AtomKind::ReorderLayers, json!({"order": ids}))?;
    finish_mutation(ctx, &result, None)
}

fn write_import_image(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let bitmap = require_object(args, "bitmap")?.clone();
    let region = parse_bbox(require_object(args, "region")?)?;
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));
    // 提交顺序协议：blob 必须已存在（12.2 校验会拒绝悬空引用）。
    let payload = json!({
        "object_id": object_id,
        "layer_id": layer_id,
        "type": "raster_patch",
        "bitmap": bitmap,
        "region": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
        "width": region.w as u64,
        "height": region.h as u64,
    });
    let result = ctx.commit(AtomKind::ImportImage, payload)?;
    let bbox = region_of(&result).or(Some(region));
    finish_mutation(ctx, &result, bbox)
}

/// 校验 `data.points` 非空（空点列会提交一个无意义的原子 —— 审计发现的静默接受）。
fn require_non_empty_points(args: &Value) -> Result<()> {
    let Some(points) = args
        .get("data")
        .and_then(|data| data.get("points"))
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    if points.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("data.points 不能为空数组"),
        ));
    }
    Ok(())
}

fn write_draw(ctx: &mut ToolContext<'_>, args: &Value, kind: AtomKind) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    // 空点列会提交一个无意义的原子（审计发现的静默接受）。
    require_non_empty_points(args)?;
    let data = require_object(args, "data")?.clone();
    validate_colors(&data)?;
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));
    let payload = json!({
        "object_id": object_id,
        "layer_id": layer_id,
        "data": data,
    });
    let result = ctx.commit(kind, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

const OBJECT_PATCH_KEYS: [&str; 6] = [
    "visible", "locked", "z_index", "layer_id", "metadata", "type",
];

fn write_update_object(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let patch = require_object(args, "patch")?.clone();
    let patch = patch.as_object().ok_or_else(|| missing("patch"))?;
    let mut atoms = Vec::new();
    for key in OBJECT_PATCH_KEYS {
        if let Some(value) = patch.get(key) {
            atoms.push(Atom::new(
                AtomKind::SetProperty,
                ctx.actor.clone(),
                ctx.session.clone(),
                json!({"object_id": object_id, "key": key, "value": value}),
            ));
        }
    }
    if atoms.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("patch 中没有可修改的对象属性"),
        ));
    }
    let changeset = Changeset::new_id();
    let results = ctx.commit_changeset(atoms, changeset.clone())?;
    let responses: Vec<Value> = results
        .iter()
        .map(|result| commit_response(result, None, None, Vec::new()))
        .collect();
    Ok(json!({
        "changeset_id": changeset,
        "atoms": responses,
        "count": results.len(),
        "head": results.last().map(|result| result.head_seq),
    }))
}

fn write_update_stroke(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let state = document_state(ctx)?;
    let object = state.objects.get(&object_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        )
        .with_object(object_id.clone())
    })?;
    let mut data = object.data.clone();
    if !data.is_object() {
        data = json!({});
    }
    // 10.3：核心参数 / preset / advanced 三层结构。
    if let Some(core) = args.get("core").and_then(Value::as_object) {
        for (key, value) in core {
            data[key] = value.clone();
        }
    }
    if let Some(preset) = optional_str(args, "preset") {
        data["preset"] = json!(preset);
    }
    if let Some(advanced) = args.get("advanced") {
        data["advanced"] = advanced.clone();
    }
    if args.get("core").is_none() && args.get("preset").is_none() && args.get("advanced").is_none()
    {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("update_stroke 需要 core / preset / advanced 至少一项"),
        ));
    }
    validate_colors(&data)?;
    let layer_id = object.layer_id.clone();
    let payload = json!({
        "object_id": object_id,
        "layer_id": layer_id,
        "data": data,
    });
    let result = ctx.commit(AtomKind::Supersede, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_move_object(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let mut payload = json!({"object_id": object_id});
    if let Some(delta) = args.get("delta") {
        payload["delta"] = delta.clone();
        let (dx, dy) = (
            delta.get("dx").and_then(Value::as_f64).unwrap_or(0.0),
            delta.get("dy").and_then(Value::as_f64).unwrap_or(0.0),
        );
        payload["transform"] = json!({"matrix": [1.0, 0.0, 0.0, 1.0, dx, dy], "pivot": [0.0, 0.0]});
    }
    if let Some(transform) = args.get("transform") {
        payload["transform"] = transform.clone();
    }
    if payload.get("transform").is_none() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("move_object 需要 delta 或 transform"),
        ));
    }
    let result = ctx.commit(AtomKind::Move, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_tombstone(ctx: &mut ToolContext<'_>, args: &Value, key: &str) -> Result<Value> {
    let id = require_str(args, key)?;
    let result = ctx.commit(AtomKind::Tombstone, json!({ key: id }))?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_history_atom(
    ctx: &mut ToolContext<'_>,
    args: &Value,
    kind: AtomKind,
    key: &str,
) -> Result<Value> {
    let target = require_str(args, key)?;
    let result = ctx.commit(kind, json!({"target": target}))?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_cancel_job(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let job_id = require_str(args, "job_id")?;
    let document = ctx.workspace.document_mut(&ctx.doc_id)?;
    let status = document.jobs_mut().cancel(&job_id, ctx.now)?;
    Ok(json!({"job_id": job_id, "status": status}))
}

fn write_set_property(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let key = require_str(args, "key")?;
    let value = args.get("value").cloned().unwrap_or(Value::Null);
    let object_id = optional_str(args, "object_id");
    let layer_id = optional_str(args, "layer_id");
    if object_id.is_none() && layer_id.is_none() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("set_property 需要 object_id 或 layer_id"),
        ));
    }
    let mut payload = json!({"key": key, "value": value});
    if let Some(object_id) = object_id {
        payload["object_id"] = json!(object_id);
    }
    if let Some(layer_id) = layer_id {
        payload["layer_id"] = json!(layer_id);
    }
    let result = ctx.commit(AtomKind::SetProperty, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_lock_layer(ctx: &mut ToolContext<'_>, args: &Value, locked: bool) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let result = ctx.commit(
        AtomKind::SetProperty,
        json!({"layer_id": layer_id, "key": "locked", "value": locked}),
    )?;
    finish_mutation(ctx, &result, None)
}

/// 调整 / 滤镜（都通过 `CreateObject` 创建，由内核的 `parse_object` 统一解释）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectKind {
    /// 调整对象（`data.adjustment_type`）。
    Adjustment,
    /// 滤镜对象（`data.filter_name`）。
    Filter,
}

impl EffectKind {
    /// 对象类型名（5.1 `type` 字段）。
    const fn object_type(self) -> &'static str {
        match self {
            Self::Adjustment => "adjustment",
            Self::Filter => "filter",
        }
    }

    /// 内核支持的名字（单一来源：`yanshi_render`）。
    fn supported(self) -> &'static [&'static str] {
        match self {
            Self::Adjustment => &yanshi_render::ADJUSTMENT_NAMES,
            Self::Filter => &yanshi_render::FILTER_NAMES,
        }
    }

    /// 参数键名（`adjustment_type` / `filter_name`）。
    const fn key(self) -> &'static str {
        match self {
            Self::Adjustment => "adjustment_type",
            Self::Filter => "filter_name",
        }
    }
}

/// 校验效果参数：内核没实现的类型/越界数值一律在工具层拒绝（5.7），不写入日志。
fn validate_effect(kind: EffectKind, name: &str, params: &Value) -> Result<()> {
    if !kind.supported().contains(&name) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "内核未实现该{}：{name}；支持 {}",
                match kind {
                    EffectKind::Adjustment => "调整类型",
                    EffectKind::Filter => "滤镜",
                },
                kind.supported().join(" / ")
            )),
        ));
    }
    let Some(object) = params.as_object() else {
        if params.is_null() {
            return Ok(());
        }
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("params 必须是 JSON 对象"),
        ));
    };
    let number = |key: &str| object.get(key).and_then(Value::as_f64);
    // 参数范围按内核的数学假设逐项校验。注意同名参数在不同效果里含义不同
    // （`radius` 对模糊滤镜是像素、对暗角是归一化半径；`amount` 对饱和度/锐化/噪点各异），
    // 因此必须**按效果名**分别限定，否则会出现「合法参数被拒」或「非法参数放行」。
    let name = name.to_owned();
    let checks: &[(&str, f64, f64)] = match (kind, name.as_str()) {
        (EffectKind::Adjustment, "brightness_contrast") => {
            &[("brightness", -1.0, 1.0), ("contrast", 0.0, 8.0)]
        }
        (EffectKind::Adjustment, "saturation") => &[("amount", 0.0, 8.0)],
        (EffectKind::Adjustment, "invert") => &[],
        (EffectKind::Adjustment, "posterize") => &[("levels", 2.0, 64.0)],
        (EffectKind::Adjustment, "vibrance") => &[("amount", 0.0, 2.0)],
        // 色彩平衡 / 分离色调的档位是 [r,g,b]，分量范围在下面的逐项检查里完成。
        (EffectKind::Adjustment, "color_balance") => &[],
        (EffectKind::Adjustment, "split_toning") => &[("balance", -1.0, 1.0), ("amount", 0.0, 1.0)],
        (EffectKind::Adjustment, "levels") => &[
            ("black", 0.0, 1.0),
            ("white", 0.0, 1.0),
            ("gamma", 0.01, 10.0),
        ],
        (EffectKind::Adjustment, "exposure") => &[("ev", -10.0, 10.0)],
        (EffectKind::Adjustment, "white_balance") => {
            &[("temperature", -1.0, 1.0), ("tint", -1.0, 1.0)]
        }
        (EffectKind::Adjustment, "curves") => &[],
        (EffectKind::Adjustment, "hsl") => &[
            ("hue", -180.0, 180.0),
            ("saturation", 0.0, 4.0),
            ("lightness", -1.0, 1.0),
        ],
        (EffectKind::Filter, "box_blur") => &[("radius", 1.0, 128.0), ("passes", 1.0, 8.0)],
        (EffectKind::Filter, "gaussian_blur") => &[("sigma", 0.05, 128.0)],
        (EffectKind::Filter, "motion_blur") => &[
            ("angle", -360.0, 360.0),
            ("distance", 0.0, 512.0),
            ("samples", 2.0, 64.0),
        ],
        (EffectKind::Filter, "sharpen") => &[("amount", 0.0, 5.0), ("radius", 1.0, 8.0)],
        (EffectKind::Filter, "noise") => &[("amount", 0.0, 1.0), ("seed", 0.0, u64::MAX as f64)],
        (EffectKind::Filter, "clarity") => &[("amount", 0.0, 2.0), ("radius", 2.0, 64.0)],
        (EffectKind::Filter, "dehaze") => &[("omega", 0.0, 1.0), ("floor", 0.02, 0.8)],
        (EffectKind::Filter, "film_grain") => &[("amount", 0.0, 1.0), ("size", 1.0, 8.0)],
        (EffectKind::Filter, "glow") => &[
            ("threshold", 0.0, 1.0),
            ("radius", 1.0, 64.0),
            ("intensity", 0.0, 4.0),
        ],
        (EffectKind::Filter, "vignette") => &[
            ("strength", 0.0, 1.0),
            ("radius", 0.0, 2.0),
            ("softness", 0.02, 2.0),
        ],
        (EffectKind::Filter, "brightness_contrast") => {
            &[("brightness", -1.0, 1.0), ("contrast", 0.0, 8.0)]
        }
        (EffectKind::Filter, "saturation") => &[("amount", 0.0, 8.0)],
        (EffectKind::Filter, "invert") => &[],
        _ => &[],
    };
    for (key, low, high) in checks {
        if let Some(value) = number(key) {
            if !(*low..=*high).contains(&value) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{name} 的 {key} 必须在 [{low}, {high}] 内，得到 {value}"
                    )),
                ));
            }
        }
    }
    if name == "levels" {
        if let Some(channel) = object.get("channel").and_then(Value::as_str) {
            // 分通道色阶：与 curves 同一套通道命名。
            if !["rgb", "r", "g", "b"].contains(&channel) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "levels 的 channel 只能是 rgb/r/g/b，得到 {channel}"
                    )),
                ));
            }
        }
        if let (Some(black), Some(white)) = (number("black"), number("white")) {
            if black >= white {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "levels 需要 black < white，得到 {black} / {white}"
                    )),
                ));
            }
        }
    }
    if name == "dehaze" {
        // 大气光必须是调用方给定的 [r,g,b]（滤镜内部不做全局估计，否则分块 != 整幅）。
        let air = object.get("air").ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("dehaze 需要 air: [r,g,b]（可用 estimate_dehaze 估计）"),
            )
        })?;
        let Some(array) = air.as_array() else {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("dehaze 的 air 必须是 [r,g,b]"),
            ));
        };
        if array.len() != 3 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("dehaze 的 air 必须是三个分量"),
            ));
        }
        for component in array {
            let Some(value) = component.as_f64() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("dehaze 的 air 分量必须是数字"),
                ));
            };
            if !(0.0..=1.0).contains(&value) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "dehaze 的 air 分量必须在 [0, 1] 内，得到 {value}"
                    )),
                ));
            }
        }
    }
    if name == "color_balance" || name == "split_toning" {
        let bands: &[&str] = if name == "color_balance" {
            &["shadows", "midtones", "highlights"]
        } else {
            &["shadows", "highlights"]
        };
        for key in bands {
            let key = *key;
            let Some(value) = object.get(key) else {
                continue;
            };
            let Some(array) = value.as_array() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{name} 的 {key} 必须是 [r,g,b]")),
                ));
            };
            if array.len() != 3 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{name} 的 {key} 必须是三个分量")),
                ));
            }
            for component in array {
                let Some(value) = component.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("{name} 的 {key} 分量必须是数字")),
                    ));
                };
                if !(-1.0..=1.0).contains(&value) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "{name} 的 {key} 分量必须在 [-1, 1] 内，得到 {value}"
                        )),
                    ));
                }
            }
        }
    }
    if name == "curves" {
        if let Some(channel) = object.get("channel").and_then(Value::as_str) {
            if !["rgb", "r", "g", "b"].contains(&channel) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "curves 的 channel 只能是 rgb/r/g/b，得到 {channel}"
                    )),
                ));
            }
        }
        if let Some(points) = object.get("points") {
            let Some(points) = points.as_array() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("curves 的 points 必须是 [[x,y], ...] 数组"),
                ));
            };
            if points.len() < 2 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("curves 至少需要两个控制点"),
                ));
            }
            for point in points {
                let Some(pair) = point.as_array() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("curves 的控制点必须是 [x,y]"),
                    ));
                };
                if pair.len() != 2 {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("curves 的控制点必须是 [x,y]"),
                    ));
                }
                for value in pair {
                    let Some(value) = value.as_f64() else {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail("curves 控制点必须是数字"),
                        ));
                    };
                    if !(0.0..=1.0).contains(&value) {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "curves 控制点必须在 [0,1] 归一化空间，得到 {value}"
                            )),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn write_add_effect(ctx: &mut ToolContext<'_>, args: &Value, kind: EffectKind) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let name = require_str(args, kind.key())?;
    let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
    validate_effect(kind, &name, &params)?;
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));
    let opacity = args
        .get("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let mut data = json!({
        kind.key(): name,
        "params": params,
        "opacity": opacity,
    });
    if let Some(label) = optional_str(args, "name") {
        data["name"] = json!(label);
    }
    // 调整/滤镜只作用于**同层下方**内容（6.6），因此缺省把效果放在同层最上方：
    // 否则默认 z 序会让它排在底图之前，表现为「加了调整却没有任何变化」。
    let z_index = match args.get("z_index").and_then(Value::as_i64) {
        Some(z_index) => z_index,
        None => {
            let state = document_state(ctx)?;
            state
                .alive_objects()
                .iter()
                .filter(|object| object.layer_id == layer_id)
                .map(|object| object.z_index)
                .max()
                .map(|max| max + 1)
                .unwrap_or(0)
        }
    };
    let result = ctx.commit(
        AtomKind::CreateObject,
        json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "type": kind.object_type(),
            "z_index": z_index,
            "data": data,
        }),
    )?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_update_effect(ctx: &mut ToolContext<'_>, args: &Value, kind: EffectKind) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let patch = require_object(args, "params")?.clone();
    let state = document_state(ctx)?;
    let object = state.objects.get(&object_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        )
        .with_object(object_id.clone())
    })?;
    if object.object_type
        != match kind {
            EffectKind::Adjustment => ObjectType::Adjustment,
            EffectKind::Filter => ObjectType::Filter,
        }
    {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{object_id} 不是{}对象", kind.object_type())),
        ));
    }
    let existing_name = object
        .data
        .get(kind.key())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut merged = object
        .data
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let (Value::Object(target), Value::Object(patch)) = (&mut merged, &patch) {
        for (key, value) in patch {
            target.insert(key.clone(), value.clone());
        }
    } else {
        merged = patch.clone();
    }
    validate_effect(kind, &existing_name, &merged)?;
    let mut data = object.data.clone();
    data["params"] = merged;
    if let Some(opacity) = args.get("opacity").and_then(Value::as_f64) {
        data["opacity"] = json!(opacity.clamp(0.0, 1.0));
    }
    let layer_id = object.layer_id.clone();
    let result = ctx.commit(
        AtomKind::Supersede,
        json!({"object_id": object_id, "layer_id": layer_id, "data": data}),
    )?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

/// 创建蒙版：内核按形状覆盖率调制图层 alpha（支持羽化与反选）。
fn write_create_mask(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let mask_id = require_str(args, "mask_id")?;
    let shape = require_object(args, "shape")?.clone();
    if shape.get("kind").and_then(Value::as_str).is_none() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("shape 必须带 kind（rect/ellipse/polygon）"),
        ));
    }
    let feather = args.get("feather").and_then(Value::as_f64).unwrap_or(0.0);
    if !(0.0..=512.0).contains(&feather) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("feather 必须在 [0, 512] 内，得到 {feather}")),
        ));
    }
    let mode = optional_str(args, "mode").unwrap_or_else(|| "new".to_owned());
    if !["new", "add", "subtract", "intersect"].contains(&mode.as_str()) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "mode 必须是 new/add/subtract/intersect，得到 {mode}"
            )),
        ));
    }
    let mut payload = json!({
        "mask_id": mask_id,
        "shape": shape,
        "feather": feather,
        "mode": mode,
        "invert": args.get("invert").and_then(Value::as_bool).unwrap_or(false),
    });
    if let Some(linked) = optional_str(args, "linked_layer") {
        payload["linked_layer"] = json!(linked);
    }
    let result = ctx.commit(AtomKind::CreateMask, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

/// 修图对象（`clone_stamp` / `heal`）：内核只实现了这两种，其余类型不放行。
fn write_retouch(ctx: &mut ToolContext<'_>, args: &Value, retouch_type: &str) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let points = require_array(args, "points")?;
    if points.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("修图至少需要一个点"),
        ));
    }
    let mut parsed: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for point in points {
        let pair = point.as_array().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("修图的 points 必须是 [[x,y], ...]"),
            )
        })?;
        let x = pair.first().and_then(Value::as_f64);
        let y = pair.get(1).and_then(Value::as_f64);
        match (x, y) {
            (Some(x), Some(y)) => parsed.push([x, y]),
            _ => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("修图的点必须是数字"),
                ))
            }
        }
    }
    // 涂抹的采样偏移由笔迹方向推导，不需要 `source_offset`；其余修图类型必须显式给出。
    // 区域外扩有上限：超出后 tile 渲染取不到源像素，分块与整幅会静默不一致。
    let size_hint = args.get("size").and_then(Value::as_f64).unwrap_or(24.0);
    let reach_limit = yanshi_render::MAX_EFFECT_PADDING as f64;
    let enforce_reach = |reach: f64, what: &str| -> Result<()> {
        if reach > reach_limit {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{what} 需要区域外扩 {reach:.0}px，超过内核上限 {reach_limit:.0}px；\
                     超出后分块渲染取不到源像素，会导致客户端与服务端结果不一致"
                )),
            ));
        }
        Ok(())
    };
    let (dx, dy) = if retouch_type == "smudge" {
        let length = args
            .get("smudge_length")
            .and_then(Value::as_f64)
            .unwrap_or(12.0);
        if !(0.5..=512.0).contains(&length) {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("smudge_length 必须在 [0.5, 512] 内，得到 {length}")),
            ));
        }
        enforce_reach(length + size_hint / 2.0 + 2.0, "smudge_length 与 size 组合")?;
        (0.0, 0.0)
    } else {
        let offset = require_array(args, "source_offset")?;
        let dx = offset.first().and_then(Value::as_f64).unwrap_or(0.0);
        let dy = offset.get(1).and_then(Value::as_f64).unwrap_or(0.0);
        if dx == 0.0 && dy == 0.0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("source_offset 不能是 [0,0]（那样只会自我复制）"),
            ));
        }
        enforce_reach(
            dx.abs().max(dy.abs()) + size_hint / 2.0 + 2.0,
            "source_offset 与 size 组合",
        )?;
        (dx, dy)
    };
    let size = args.get("size").and_then(Value::as_f64).unwrap_or(24.0);
    if !(1.0..=1024.0).contains(&size) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("size 必须在 [1, 1024] 内，得到 {size}")),
        ));
    }
    let hardness = args.get("hardness").and_then(Value::as_f64).unwrap_or(0.6);
    let opacity = args.get("opacity").and_then(Value::as_f64).unwrap_or(1.0);
    for (key, value) in [("hardness", hardness), ("opacity", opacity)] {
        if !(0.0..=1.0).contains(&value) {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{key} 必须在 [0, 1] 内，得到 {value}")),
            ));
        }
    }
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));
    let z_index = match args.get("z_index").and_then(Value::as_i64) {
        Some(z_index) => z_index,
        None => {
            let state = document_state(ctx)?;
            state
                .alive_objects()
                .iter()
                .filter(|object| object.layer_id == layer_id)
                .map(|object| object.z_index)
                .max()
                .map(|max| max + 1)
                .unwrap_or(0)
        }
    };
    let result = ctx.commit(
        AtomKind::Retouch,
        json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "z_index": z_index,
            "data": {
                "retouch_type": retouch_type,
                "points": parsed,
                "source_offset": [dx, dy],
                "smudge_length": args.get("smudge_length").and_then(Value::as_f64).unwrap_or(12.0),
                "size": size,
                "hardness": hardness,
                "opacity": opacity,
            },
        }),
    )?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

/// 图章补丁：抓取源区域像素 → 存 blob（**先于**原子，6.3）→ 建 `raster_patch` 对象。
fn write_patch(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let source = parse_bbox(require_object(args, "source_region")?)?;
    if source.w < 1.0 || source.h < 1.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("source_region 的宽高必须 ≥ 1"),
        ));
    }
    let target = require_array(args, "target")?;
    let tx = target.first().and_then(Value::as_f64);
    let ty = target.get(1).and_then(Value::as_f64);
    let (Some(tx), Some(ty)) = (tx, ty) else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("target 必须是 [x, y]"),
        ));
    };
    let opacity = args.get("opacity").and_then(Value::as_f64).unwrap_or(1.0);
    if !(0.0..=1.0).contains(&opacity) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("opacity 必须在 [0, 1] 内，得到 {opacity}")),
        ));
    }
    // 1) 抓取源像素（原始 RGBA8；不写渲染缓存）。
    let (width, height, pixels) = ctx.workspace.render_region_raw(&ctx.doc_id, source)?;
    // 2) blob 先行：提交顺序协议要求原子写入前引用必须已存在（12.2）。
    let blob_hash = ctx.workspace.store().put(&pixels)?;
    // 3) 落到目标位置：`region` 即落点矩形（内核据此放置）。
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));
    let z_index = match args.get("z_index").and_then(Value::as_i64) {
        Some(z_index) => z_index,
        None => {
            let state = document_state(ctx)?;
            state
                .alive_objects()
                .iter()
                .filter(|object| object.layer_id == layer_id)
                .map(|object| object.z_index)
                .max()
                .map(|max| max + 1)
                .unwrap_or(0)
        }
    };
    let result = ctx.commit(
        AtomKind::CreateObject,
        json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "type": "raster_patch",
            "z_index": z_index,
            "data": {
                "bitmap": {
                    "blob_hash": blob_hash.to_string(),
                    "mime_type": yanshi_render::RAW_RGBA_MIME,
                },
                "region": {"x": tx, "y": ty, "w": width, "h": height},
                "width": width,
                "height": height,
                "opacity": opacity,
            },
        }),
    )?;
    let region = region_of(&result).or(Some(Bbox::new(tx, ty, width as f64, height as f64)));
    finish_mutation(ctx, &result, region)
}

/// 估计去雾参数（只读）：整幅渲染后按暗通道先验给出建议大气光。
///
/// 之所以做成**独立只读工具**而不是让滤镜内部估计：全局统计量若在滤镜内按区域计算，
/// 同一像素在分块渲染与整幅渲染下结果不同，「分块 == 整幅」立刻失效（此前已踩过）。
/// 估计值由调用方写进原子参数，因此渲染始终是逐像素确定性的；
/// 这个两步式流程也天然适配 Phase 4b 的「估计 → 作为 patch 应用」。
fn read_estimate_dehaze(ctx: &mut ToolContext<'_>) -> Result<Value> {
    let (width, height) = {
        let state = document_state(ctx)?;
        (state.width, state.height)
    };
    let (raw_width, raw_height, pixels) = ctx.workspace.render_region_raw(
        &ctx.doc_id,
        Bbox::new(0.0, 0.0, width as f64, height as f64),
    )?;
    let (air, dark_mean) = yanshi_render::estimate_atmospheric_light(
        raw_width, raw_height, &pixels,
    )
    .ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("像素数与尺寸不匹配，无法估计"),
        )
    })?;
    // 雾越浓（暗通道均值越高）建议越强，但留出保守上限。
    let suggested_omega = (0.6 + dark_mean * 0.6).clamp(0.6, 0.95);
    Ok(json!({
        "air": air,
        "dark_channel_mean": dark_mean,
        "suggested": {"omega": suggested_omega, "floor": 0.1},
        "note": "把 air/omega/floor 传给 add_filter {filter_name: \"dehaze\"}；渲染逐像素确定性",
    }))
}

/// 液化（`push` / `twirl` / `pinch`）：创建 `liquify` 对象。
fn write_liquify(ctx: &mut ToolContext<'_>, args: &Value, mode: &str) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let points = require_array(args, "points")?;
    if points.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("liquify 至少需要一个点"),
        ));
    }
    let mut parsed: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for point in &points {
        let pair = point.as_array().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("liquify 的 points 必须是 [[x,y], ...]"),
            )
        })?;
        match (
            pair.first().and_then(Value::as_f64),
            pair.get(1).and_then(Value::as_f64),
        ) {
            (Some(x), Some(y)) => parsed.push([x, y]),
            _ => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("liquify 的点必须是数字"),
                ))
            }
        }
    }
    // 只有推力模式需要方向；旋转/收缩由笔迹点位置决定。
    let (dx, dy) = if mode == "push" {
        let direction = require_array(args, "direction")?;
        let dx = direction.first().and_then(Value::as_f64).unwrap_or(0.0);
        let dy = direction.get(1).and_then(Value::as_f64).unwrap_or(0.0);
        if dx == 0.0 && dy == 0.0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("direction 不能是 [0,0]"),
            ));
        }
        (dx, dy)
    } else {
        (0.0, 0.0)
    };
    let size = args.get("size").and_then(Value::as_f64).unwrap_or(80.0);
    if !(2.0..=2048.0).contains(&size) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("size 必须在 [2, 2048] 内，得到 {size}")),
        ));
    }
    let strength = args.get("strength").and_then(Value::as_f64).unwrap_or(0.5);
    // 收缩/膨胀需要符号，因此 pinch 允许负值。
    let (low, high) = if mode == "pinch" {
        (-2.0, 2.0)
    } else {
        (0.0, 2.0)
    };
    if !(low..=high).contains(&strength) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "{mode} 的 strength 必须在 [{low}, {high}] 内，得到 {strength}"
            )),
        ));
    }
    let liquify_reach = size / 2.0 + strength * size + 2.0;
    if liquify_reach > yanshi_render::MAX_EFFECT_PADDING as f64 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "size 与 strength 组合需要区域外扩 {liquify_reach:.0}px，超过内核上限 {}px",
                yanshi_render::MAX_EFFECT_PADDING
            )),
        ));
    }
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));
    let z_index = match args.get("z_index").and_then(Value::as_i64) {
        Some(z_index) => z_index,
        None => {
            let state = document_state(ctx)?;
            state
                .alive_objects()
                .iter()
                .filter(|object| object.layer_id == layer_id)
                .map(|object| object.z_index)
                .max()
                .map(|max| max + 1)
                .unwrap_or(0)
        }
    };
    let result = ctx.commit(
        AtomKind::Liquify,
        json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "z_index": z_index,
            "data": {
                "liquify_type": mode,
                "points": parsed,
                "size": size,
                "strength": strength,
                "direction": [dx, dy],
            },
        }),
    )?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn read_list_effects(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_filter = optional_str(args, "layer_id");
    let state = document_state(ctx)?;
    let effects: Vec<Value> = state
        .alive_objects()
        .into_iter()
        .filter(|object| {
            matches!(
                object.object_type,
                ObjectType::Adjustment | ObjectType::Filter
            )
        })
        .filter(|object| {
            layer_filter
                .as_deref()
                .is_none_or(|id| object.layer_id == id)
        })
        .map(|object| {
            json!({
                "object_id": object.id,
                "layer_id": object.layer_id,
                "type": object.object_type,
                "name": object.data.get("name"),
                "adjustment_type": object.data.get("adjustment_type"),
                "filter_name": object.data.get("filter_name"),
                "params": object.data.get("params"),
                "opacity": object.data.get("opacity"),
            })
        })
        .collect();
    Ok(json!({
        "effects": effects,
        "count": effects.len(),
        "supported": {
            "adjustments": yanshi_render::ADJUSTMENT_NAMES,
            "filters": yanshi_render::FILTER_NAMES,
        },
    }))
}

/// 记录一条建议（12.6：`suggest` 原子**包含 patch**）。
///
/// patch 采用「工具调用序列」表示（`[{\"tool\": ..., \"arguments\": {...}}]`）：
/// 这样 `accept_suggestion` 只需按序重新执行它们即可实现「→ reapply」，
/// 且与 AI 的输出形式天然一致。这里只校验形状，工具名与参数在**接受时**校验
/// —— 因为那时才知道当前 profile 暴露了哪些工具。
fn write_suggest(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let patch = require_array(args, "patch")?;
    if patch.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("patch 不能为空"),
        ));
    }
    if patch.len() > 64 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("patch 步骤过多（{} > 64）", patch.len())),
        ));
    }
    for (index, step) in patch.iter().enumerate() {
        let Some(object) = step.as_object() else {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("patch[{index}] 必须是对象")),
            ));
        };
        let Some(tool) = object.get("tool").and_then(Value::as_str) else {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("patch[{index}] 缺少 tool")),
            ));
        };
        if tool.trim().is_empty() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("patch[{index}] 的 tool 为空")),
            ));
        }
        if let Some(arguments) = object.get("arguments") {
            if !arguments.is_object() {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("patch[{index}] 的 arguments 必须是对象")),
                ));
            }
        }
    }
    // 优先级：0..9，缺省 5（中等）。用于 list_suggestions 排序，便于人工先看要紧的。
    let priority = match args.get("priority").and_then(Value::as_i64) {
        None => 5,
        Some(value) if (0..=9).contains(&value) => value,
        Some(value) => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("priority 必须在 0..9 内，得到 {value}")),
            ))
        }
    };
    let result = ctx.commit(
        AtomKind::Suggest,
        json!({
            "patch": patch,
            "annotation_id": optional_str(args, "annotation_id"),
            "summary": optional_str(args, "summary"),
            "priority": priority,
        }),
    )?;
    Ok(json!({
        "suggestion_id": result.atom_id,
        "seq": result.seq,
        "steps": patch.len(),
        "priority": priority,
    }))
}

/// 所有已实现 profile 的并集（与 HTTP 服务端默认集一致）。
fn all_implemented_profiles() -> Vec<Profile> {
    vec![
        Profile::Core,
        Profile::History,
        Profile::Annotation,
        Profile::Collab,
        Profile::Structure,
        Profile::Retouch,
    ]
}

/// 建议预览（不应用）：逐步校验并归类，供人工/AI 在应用前审阅。
///
/// 只做**静态**判断：工具是否存在、是否 mutating、参数是否通过校验、目标是谁、
/// 属于哪类变更。精确的 dirty 范围需要在应用时才知道（内核按对象与效果计算），
/// 因此这里如实说明，而不是猜一个数字。
fn read_preview_suggestion(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let suggestion_id = optional_str(args, "suggestion_id");
    let patch = if let Some(patch) = args.get("patch") {
        patch.as_array().cloned().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("patch 必须是数组"),
            )
        })?
    } else {
        let Some(suggestion_id) = suggestion_id.clone() else {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("需要 suggestion_id 或 patch"),
            ));
        };
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let atom = document.log().get(&suggestion_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("建议 {suggestion_id} 不在日志中")),
            )
        })?;
        if atom.kind != AtomKind::Suggest {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{suggestion_id} 不是建议（suggest）原子")),
            ));
        }
        atom.payload
            .get("patch")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let registry = ToolRegistry::with_profiles(&all_implemented_profiles());
    let mut steps = Vec::new();
    let mut invalid = 0usize;
    let mut aggregate_affects = "none";
    let mut aggregate_region: Option<Bbox> = None;
    // 依赖顺序模拟：按 patch 顺序跟踪「此刻已知的图层/对象」，
    // 这样能挡住「先画后建图层」这类会在**接受过程中**失败、留下部分应用状态的补丁。
    let (mut known_layers, mut known_objects) = {
        let state = document_state(ctx)?;
        let layers: BTreeSet<String> = state
            .alive_layers()
            .iter()
            .map(|layer| layer.id.clone())
            .collect();
        let objects: BTreeSet<String> = state
            .alive_objects()
            .iter()
            .map(|object| object.id.clone())
            .collect();
        (layers, objects)
    };
    for (index, step) in patch.iter().enumerate() {
        let tool = step
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let step_args = step.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let (affects, region) = estimate_step_impact(&tool, &step_args);
        let mut entry = json!({
            "index": index,
            "tool": tool,
            "class": tool_class(&tool),
            "affects": affects,
            "estimated_region": region.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
            "layer_id": step_args.get("layer_id").cloned().unwrap_or(Value::Null),
            "object_id": step_args.get("object_id").cloned().unwrap_or(Value::Null),
        });
        // 汇总：document > layer > region。
        match affects {
            "document" => aggregate_affects = "document",
            "layer" if aggregate_affects != "document" => aggregate_affects = "layer",
            "region" if aggregate_affects == "none" => aggregate_affects = "region",
            _ => {}
        }
        if let Some(bbox) = region {
            aggregate_region = Some(match aggregate_region {
                Some(existing) => existing.union(&bbox),
                None => bbox,
            });
        }
        // 与接受路径共用同一个校验函数（未知工具、只读、参数、效果范围、引用顺序）。
        match check_single_step(
            &registry,
            &tool,
            &step_args,
            &mut known_layers,
            &mut known_objects,
        ) {
            Ok(()) => entry["valid"] = json!(true),
            Err(error) => {
                invalid += 1;
                entry["valid"] = json!(false);
                entry["error"] = json!(format!("{:?}", error.context.detail));
            }
        }
        steps.push(entry);
    }
    Ok(json!({
        "suggestion_id": suggestion_id,
        "steps": steps,
        "total_steps": patch.len(),
        "invalid_steps": invalid,
        "applicable": invalid == 0 && !patch.is_empty(),
        "estimated_dirty": aggregate_affects,
        "estimated_region": aggregate_region.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
        "notes": "静态检查：工具存在性、mutating、参数形状与效果类取值范围；\
                  个别工具的运行期限制（如外扩上限）在应用时最终判定",
    }))
}

/// 预览里补充执行的「效果类参数」校验（与处理器共用同一函数，避免两条标准漂移）。
fn preview_effect_check(tool: &str, args: &Value) -> Result<()> {
    let (kind, key) = match tool {
        "add_adjustment" => (EffectKind::Adjustment, "adjustment_type"),
        "add_filter" => (EffectKind::Filter, "filter_name"),
        _ => return Ok(()),
    };
    let Some(name) = args.get(key).and_then(Value::as_str) else {
        return Ok(());
    };
    let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
    validate_effect(kind, name, &params)
}

/// 建议的作用域：整层/整文档级别、区域并集、以及涉及的图层与对象 id。
struct SuggestScope {
    /// `document` / `layer` / `region` / `none`。
    affects: &'static str,
    /// 区域并集。
    region: Option<Bbox>,
    /// 涉及的图层 id。
    layers: BTreeSet<String>,
    /// 涉及的对象 id。
    objects: BTreeSet<String>,
}

/// 由 patch 推导作用域（复用逐步的影响预估，保证与预览口径一致）。
fn suggestion_scope(patch: &[Value]) -> SuggestScope {
    let mut scope = SuggestScope {
        affects: "none",
        region: None,
        layers: BTreeSet::new(),
        objects: BTreeSet::new(),
    };
    for step in patch {
        let Some(object) = step.as_object() else {
            continue;
        };
        let tool = object
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = object
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let (affects, region) = estimate_step_impact(tool, &args);
        match affects {
            "document" => scope.affects = "document",
            "layer" if scope.affects != "document" => scope.affects = "layer",
            "region" if scope.affects == "none" => scope.affects = "region",
            _ => {}
        }
        if let Some(bbox) = region {
            scope.region = Some(match scope.region {
                Some(existing) => existing.union(&bbox),
                None => bbox,
            });
        }
        if let Some(layer_id) = args.get("layer_id").and_then(Value::as_str) {
            scope.layers.insert(layer_id.to_owned());
        }
        if let Some(object_id) = args.get("object_id").and_then(Value::as_str) {
            scope.objects.insert(object_id.to_owned());
        }
    }
    scope
}

/// 两条建议是否冲突：同图层（层/文档级）、同对象、或区域相交。
fn scopes_conflict(a: &SuggestScope, b: &SuggestScope) -> Option<String> {
    if a.affects == "document" || b.affects == "document" {
        return Some("两者之一会改变图层结构（整文档失效）".to_owned());
    }
    if !a.objects.is_disjoint(&b.objects) {
        return Some("两者修改同一个对象".to_owned());
    }
    let layer_level = |scope: &SuggestScope| scope.affects == "layer";
    if layer_level(a) || layer_level(b) {
        if !a.layers.is_disjoint(&b.layers) {
            return Some("同图层上存在作用于整层的改动（调整/滤镜）".to_owned());
        }
        // 一方整层、另一方同层区域 → 仍然冲突。
        if layer_level(a) && !a.layers.is_disjoint(&b.layers) {
            return Some("一方作用于整层，另一方也改动该层".to_owned());
        }
    }
    match (a.region, b.region) {
        (Some(left), Some(right)) if left.intersects(&right) => {
            Some("两者的影响区域相交".to_owned())
        }
        _ => None,
    }
}

/// 单步校验：工具存在且 mutating、参数形状、效果类取值范围、引用顺序。
///
/// 预览与接受**共用这一个函数**，因此两条路径的标准不会分叉。
fn check_single_step(
    registry: &ToolRegistry,
    tool: &str,
    args: &Value,
    layers: &mut BTreeSet<String>,
    objects: &mut BTreeSet<String>,
) -> Result<()> {
    let spec = registry.get(tool).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("未知工具 {tool}")),
        )
    })?;
    if !spec.mutating {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{tool} 是只读工具，不能作为补丁步骤")),
        ));
    }
    validate_args(spec, args)?;
    preview_effect_check(tool, args)?;
    check_step_order(tool, args, layers, objects)
}

/// 接受前的**全量预检**：任一步不合法就整体拒绝，避免「部分应用」。
///
/// 注意这不能覆盖**运行期**失败（例如折叠层的其它前置条件）；它覆盖的是
/// 未知工具、只读步骤、参数非法与引用顺序这四类最常见问题——
/// 也就是预览已经能报出来的那些。做完这轮预检后，接受要么全部落地，要么一步都没写。
fn preflight_patch(ctx: &mut ToolContext<'_>, patch: &[Value]) -> Result<()> {
    let registry = ToolRegistry::with_profiles(&all_implemented_profiles());
    let (mut layers, mut objects) = {
        let state = document_state(ctx)?;
        (
            state
                .alive_layers()
                .iter()
                .map(|layer| layer.id.clone())
                .collect::<BTreeSet<String>>(),
            state
                .alive_objects()
                .iter()
                .map(|object| object.id.clone())
                .collect::<BTreeSet<String>>(),
        )
    };
    for step in patch {
        let tool = step.get("tool").and_then(Value::as_str).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("补丁步骤缺少 tool"),
            )
        })?;
        let args = step.get("arguments").cloned().unwrap_or_else(|| json!({}));
        check_single_step(&registry, tool, &args, &mut layers, &mut objects)?;
    }
    Ok(())
}

/// 按 patch 顺序检查引用有效性，并更新「已知图层/对象」集合。
///
/// 规则：`create_layer`/`create_*`/`draw_*` 等创建型步骤把目标加入集合；
/// 其余引用 `layer_id`/`object_id` 的步骤要求目标**此刻已存在**；
/// `delete_*` 会把目标移出集合（后续再引用即报错）。
fn check_step_order(
    tool: &str,
    args: &Value,
    layers: &mut BTreeSet<String>,
    objects: &mut BTreeSet<String>,
) -> Result<()> {
    let layer_id = args.get("layer_id").and_then(Value::as_str);
    let object_id = args.get("object_id").and_then(Value::as_str);
    let creates_layer = tool == "create_layer";
    let creates_object = matches!(
        tool,
        "create_object"
            | "import_image"
            | "draw_stroke"
            | "draw_shape"
            | "draw_text"
            | "fill"
            | "erase"
            | "retouch"
            | "liquify"
            | "clone_stamp"
            | "heal_stamp"
            | "smudge"
            | "patch"
            | "add_adjustment"
            | "add_filter"
    );
    let deletes_layer = tool == "delete_layer";
    let deletes_object = matches!(tool, "delete_object" | "tombstone");

    if !creates_layer && !deletes_layer {
        if let Some(layer_id) = layer_id {
            if !layers.contains(layer_id) {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!(
                        "该步骤引用的图层 {layer_id} 在此时尚未创建（补丁顺序问题）"
                    )),
                ));
            }
        }
    }
    if !creates_object && !deletes_object {
        if let Some(object_id) = object_id {
            if !objects.contains(object_id) {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!(
                        "该步骤引用的对象 {object_id} 在此时不存在（补丁顺序问题）"
                    )),
                ));
            }
        }
    }
    if creates_layer {
        if let Some(layer_id) = layer_id {
            layers.insert(layer_id.to_owned());
        }
    }
    if creates_object {
        if let Some(object_id) = object_id {
            objects.insert(object_id.to_owned());
        }
    }
    if deletes_layer {
        if let Some(layer_id) = layer_id {
            layers.remove(layer_id);
        }
    }
    if deletes_object {
        if let Some(object_id) = object_id {
            objects.remove(object_id);
        }
    }
    Ok(())
}

/// 预估某一步骤的影响范围（按内核真实的 dirty 语义推导）。
///
/// 返回 `(affects, region)`：`affects` ∈ `document` / `layer` / `region` / `unknown`，
/// `region` 为可推导时的影响包围盒。这是**预估**而不是实测：精确范围由内核在应用时
/// 按对象与效果计算，这里只依据「效果类作用于整层」「几何类按包围盒」等既定语义给出，
/// 目的是让审阅者在应用前知道量级（例如「这一步会改整层」）。
fn estimate_step_impact(tool: &str, args: &Value) -> (&'static str, Option<Bbox>) {
    let number = |key: &str| args.get(key).and_then(Value::as_f64);
    // 参数既可能平铺（`points`/`bbox`），也可能嵌在 `data` 里（工具调用的常见形状）。
    let data = args.get("data");
    let lookup = |key: &str| -> Option<&Value> {
        args.get(key)
            .or_else(|| data.and_then(|data| data.get(key)))
    };
    let points_bbox = |margin: f64| -> Option<Bbox> {
        let points = lookup("points")?.as_array()?;
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for point in points {
            let pair = point.as_array()?;
            let x = pair.first()?.as_f64()?;
            let y = pair.get(1)?.as_f64()?;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        if !min_x.is_finite() {
            return None;
        }
        let margin = margin.max(0.0);
        Some(Bbox::new(
            min_x - margin,
            min_y - margin,
            (max_x - min_x) + margin * 2.0,
            (max_y - min_y) + margin * 2.0,
        ))
    };
    let region_bbox = || -> Option<Bbox> {
        args.get("region")
            .and_then(Bbox::from_value)
            .or_else(|| lookup("bbox").and_then(Bbox::from_value))
            .or_else(|| {
                // 形状对象的 bbox 常在 `data.geometry.bbox`。
                data.and_then(|data| data.get("geometry"))
                    .and_then(|geometry| geometry.get("bbox"))
                    .and_then(Bbox::from_value)
            })
            .or_else(|| {
                let (x, y) = (number("x")?, number("y")?);
                Some(Bbox::new(x, y, number("w")?, number("h")?))
            })
    };
    if matches!(tool, "create_layer" | "reorder_layers" | "delete_layer") {
        // 图层结构变化按内核语义是整文档失效。
        return ("document", None);
    }
    if tool.starts_with("add_adjustment")
        || tool.starts_with("add_filter")
        || tool.starts_with("update_adjustment")
        || tool.starts_with("update_filter")
    {
        // 6.6：调整/滤镜作用于同层下方全部内容 → 整层结构 dirty。
        return ("layer", None);
    }
    if tool == "set_property" {
        let key = args.get("key").and_then(Value::as_str).unwrap_or_default();
        return match key {
            // 这些属性会改变依赖闭包 → 整层。
            "z_index" | "visible" | "layer_id" | "mask_id" | "style_id" | "type" => ("layer", None),
            // 其余属性（如元数据）按对象范围。
            _ => ("region", object_bbox_from_args(args)),
        };
    }
    if tool.starts_with("clone_stamp") || tool.starts_with("heal_stamp") || tool == "smudge" {
        // 修图：笔迹范围 + 采样偏移/涂抹距离 + 笔刷半径。
        let size = number("size").unwrap_or(24.0);
        let reach = number("source_offset")
            .map(f64::abs)
            .unwrap_or(0.0)
            .max(number("smudge_length").unwrap_or(0.0));
        return ("region", points_bbox(size / 2.0 + reach + 2.0));
    }
    if tool.starts_with("liquify") {
        let size = number("size").unwrap_or(80.0);
        let strength = number("strength").unwrap_or(0.5).abs().min(2.0);
        return ("region", points_bbox(size / 2.0 + strength * size + 2.0));
    }
    if tool == "patch" {
        let width = args
            .get("source_region")
            .and_then(Bbox::from_value)
            .map(|bbox| bbox.w)
            .unwrap_or(0.0);
        let height = args
            .get("source_region")
            .and_then(Bbox::from_value)
            .map(|bbox| bbox.h)
            .unwrap_or(0.0);
        let target = args.get("target").and_then(Value::as_array);
        return match target {
            Some(pair) => {
                let x = pair.first().and_then(Value::as_f64).unwrap_or(0.0);
                let y = pair.get(1).and_then(Value::as_f64).unwrap_or(0.0);
                ("region", Some(Bbox::new(x, y, width, height)))
            }
            None => ("region", None),
        };
    }
    if tool.starts_with("draw_") || tool == "fill" || tool == "erase" || tool == "import_image" {
        let size = number("size").unwrap_or(0.0);
        return ("region", points_bbox(size / 2.0 + 2.0).or_else(region_bbox));
    }
    if tool.starts_with("move_") || tool == "transform" || tool.starts_with("delete_") {
        return ("region", object_bbox_from_args(args));
    }
    if tool.starts_with("create_mask") {
        // 蒙版羽化会扩展到整层。
        return ("layer", None);
    }
    ("unknown", None)
}

/// 从 `object_id` 参数推导对象包围盒（拿不到就用 `region`/`bbox` 参数兜底）。
fn object_bbox_from_args(args: &Value) -> Option<Bbox> {
    args.get("region")
        .and_then(Bbox::from_value)
        .or_else(|| args.get("bbox").and_then(Bbox::from_value))
}

/// 补丁步骤的粗分类（供预览展示；精确影响范围在应用时由内核计算）。
fn tool_class(tool: &str) -> &'static str {
    if tool.starts_with("add_adjustment")
        || tool.starts_with("add_filter")
        || tool.starts_with("update_adjustment")
        || tool.starts_with("update_filter")
    {
        "effect"
    } else if tool.starts_with("clone_stamp")
        || tool.starts_with("heal_stamp")
        || tool == "smudge"
        || tool == "patch"
        || tool.starts_with("liquify")
    {
        "retouch"
    } else if tool.starts_with("draw_")
        || tool == "fill"
        || tool == "erase"
        || tool.starts_with("move_")
        || tool == "transform"
    {
        "geometry"
    } else if tool.starts_with("create_")
        || tool.starts_with("reorder_")
        || tool == "set_property"
        || tool.starts_with("delete_")
    {
        "structure"
    } else {
        "other"
    }
}

/// 接受建议：按序重放 patch（设计 12.6：`accept_suggestion → reapply`）。
///
/// 重放走**同一张工具分发表**，因此与人工操作完全同路径（同样的校验、dirty 传播与响应）。
/// 只允许「会产生状态效果」的步骤：把只读工具塞进 patch 没有意义，也容易出现意外行为。
fn write_accept_suggestion(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let suggestion_id = require_str(args, "suggestion_id")?;
    accept_one(ctx, &suggestion_id)
}

/// 批量接受：逐条尝试，**不因个别失败中断**，逐条报告结果。
fn write_accept_suggestions(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let ids = require_array(args, "suggestion_ids")?;
    if ids.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("suggestion_ids 不能为空"),
        ));
    }
    if ids.len() > 64 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("一次最多接受 64 条建议（收到 {}）", ids.len())),
        ));
    }
    let mut accepted = 0usize;
    let mut results = Vec::new();
    for value in ids {
        let Some(id) = value.as_str() else {
            results.push(json!({"suggestion_id": value, "ok": false, "error": "id 必须是字符串"}));
            continue;
        };
        match accept_one(ctx, id) {
            Ok(result) => {
                accepted += 1;
                results.push(json!({
                    "suggestion_id": id,
                    "ok": true,
                    "applied_atom_ids": result["applied_atom_ids"],
                    "resolved_annotations": result["resolved_annotations"],
                }));
            }
            Err(error) => results.push(json!({
                "suggestion_id": id,
                "ok": false,
                "error_code": error.code.as_str(),
                "detail": error.context.detail,
            })),
        }
    }
    Ok(json!({
        "results": results,
        "accepted": accepted,
        "failed": results.len() - accepted,
        "pending_annotations": 0,
    }))
}

/// 接受单条建议：查找 patch → 重放 → 提交接受原子 → 更新关联标注。
fn accept_one(ctx: &mut ToolContext<'_>, suggestion_id: &str) -> Result<Value> {
    let suggestion_id = suggestion_id.to_owned();
    // 1) 取出建议的 patch（必须是一条 suggest 原子）。
    let (patch, annotation_id) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let atom = document.log().get(&suggestion_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("建议 {suggestion_id} 不在日志中")),
            )
            .with_atom(suggestion_id.clone())
        })?;
        if atom.kind != AtomKind::Suggest {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{suggestion_id} 是 {} 原子，不是建议（suggest）",
                    atom.kind
                )),
            ));
        }
        let patch = atom
            .payload
            .get("patch")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let annotation_id = atom
            .payload
            .get("annotation_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        (patch, annotation_id)
    };
    if patch.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("建议没有可重放的步骤"),
        ));
    }
    // 1.5) 状态闸门（设计未规定，此处记录决定）：
    //   * 已接受的建议 → **不再重复施加效果**，返回既有结果（`already_accepted: true`）。
    //     理由：accept 的语义是「采纳这份提议」，重复采纳不该把效果叠两遍；
    //     批量接受场景下 AI 很容易重复提交同一个 id，这属于必须防的footgun。
    //   * 已拒绝的建议 → 拒绝接受（`precondition_failed`），避免状态自相矛盾。
    let (accepted_by, rejected_by) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let mut accepted_by = None;
        let mut rejected_by = None;
        for atom in document.log().iter() {
            let target = atom.payload.get("target_atom_id").and_then(Value::as_str);
            if target != Some(suggestion_id.as_str()) {
                continue;
            }
            match atom.kind {
                AtomKind::AcceptSuggestion => accepted_by = Some(atom.id.clone()),
                AtomKind::RejectSuggestion => rejected_by = Some(atom.id.clone()),
                _ => {}
            }
        }
        (accepted_by, rejected_by)
    };
    if let Some(accept_atom) = accepted_by {
        return Ok(json!({
            "suggestion_id": suggestion_id,
            "applied_atom_ids": Vec::<String>::new(),
            "accept_atom_id": accept_atom,
            "resolved_annotations": Vec::<Value>::new(),
            "already_accepted": true,
            "note": "该建议此前已被接受，本次不重复施加效果",
        }));
    }
    if let Some(reject_atom) = rejected_by {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "建议 {suggestion_id} 已被拒绝（原子 {reject_atom}），不能接受已拒绝的建议"
            )),
        ));
    }
    // 2) **全量预检**：任何一步不合法就整体拒绝，保证「要么全部落地、要么一步没写」。
    //    注册表用「所有已实现 profile 的并集」，与 HTTP 服务端默认集一致。
    preflight_patch(ctx, &patch)?;
    let registry = ToolRegistry::with_profiles(&all_implemented_profiles());
    // 3) 逐条重放。
    let mut applied: Vec<String> = Vec::new();
    for (index, step) in patch.iter().enumerate() {
        let tool = step.get("tool").and_then(Value::as_str).unwrap_or_default();
        let step_args = step.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let spec = registry.get(tool).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("patch[{index}] 引用了未知工具 {tool}")),
            )
        })?;
        validate_args(spec, &step_args)?;
        let value = dispatch(spec, ctx, &step_args)?;
        if let Some(atom_id) = value.get("atom_id").and_then(Value::as_str) {
            applied.push(atom_id.to_owned());
        }
    }
    // 4) 提交接受原子并更新关联标注。
    let result = ctx.commit(
        AtomKind::AcceptSuggestion,
        json!({
            "target_atom_id": suggestion_id,
            "applied_atom_ids": applied,
            "annotation_id": annotation_id,
        }),
    )?;
    let accept_atom = result.atom_id.clone();
    let (resolved, pending) = {
        let document = ctx.workspace.document_mut(&ctx.doc_id).map_err(|_| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let filter = AnnotationFilter {
            status: Some(AnnotationStatus::Pending),
            suggestion_id: Some(suggestion_id.clone()),
            ..AnnotationFilter::default()
        };
        let ids: Vec<String> = document
            .annotations()
            .list(&filter)
            .into_iter()
            .map(|annotation| annotation.id)
            .collect();
        let mut resolved = Vec::new();
        for id in ids {
            resolved.push(document.annotations_mut().resolve(
                &id,
                Some(accept_atom.clone()),
                ctx.now,
            )?);
        }
        (resolved, document.annotations().pending_count())
    };
    Ok(json!({
        "suggestion_id": suggestion_id,
        "applied_atom_ids": applied,
        "accept_atom_id": accept_atom,
        "resolved_annotations": resolved,
        "pending_annotations": pending,
    }))
}

/// 拒绝建议：提交 `reject_suggestion` 原子记录原因，并把引用它的标注置为 rejected。
fn write_reject_suggestion(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let suggestion_id = require_str(args, "suggestion_id")?;
    let reason = optional_str(args, "reason").unwrap_or_default();
    reject_one(ctx, &suggestion_id, &reason)
}

/// 批量拒绝：与批量接受对称 —— **逐条尝试、个别失败不中断**，逐条返回结果。
fn write_reject_suggestions(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let ids = require_array(args, "suggestion_ids")?;
    if ids.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("suggestion_ids 不能为空"),
        ));
    }
    if ids.len() > 64 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("一次最多拒绝 64 条建议（收到 {}）", ids.len())),
        ));
    }
    let reason = optional_str(args, "reason").unwrap_or_default();
    let mut rejected = 0usize;
    let mut results = Vec::new();
    for value in ids {
        let Some(id) = value.as_str() else {
            results.push(json!({"suggestion_id": value, "ok": false, "error": "id 必须是字符串"}));
            continue;
        };
        match reject_one(ctx, id, &reason) {
            Ok(result) => {
                rejected += 1;
                results.push(json!({
                    "suggestion_id": id,
                    "ok": true,
                    "already_rejected": result["already_rejected"],
                    "rejected_annotations": result["rejected_annotations"],
                }));
            }
            Err(error) => results.push(json!({
                "suggestion_id": id,
                "ok": false,
                "error_code": error.code.as_str(),
                "detail": error.context.detail,
            })),
        }
    }
    Ok(json!({
        "results": results,
        "rejected": rejected,
        "failed": results.len() - rejected,
    }))
}

/// 拒绝单条建议（含状态闸门，与接受侧镜像）。
fn reject_one(ctx: &mut ToolContext<'_>, suggestion_id: &str, reason: &str) -> Result<Value> {
    // 先确认这确实是一条**已存在**的 suggest 原子。
    // （此前的实现不校验存在性：拒绝一个拼错的 id 也会"成功"并写入无意义原子 ✗ —— 由批量拒绝的
    //   测试发现：未知 id 竟然返回 ok。）
    {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let atom = document.log().get(suggestion_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("建议 {suggestion_id} 不在日志中")),
            )
            .with_atom(suggestion_id.to_owned())
        })?;
        if atom.kind != AtomKind::Suggest {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{suggestion_id} 是 {} 原子，不是建议（suggest）",
                    atom.kind
                )),
            ));
        }
    }
    // 状态闸门（与 `accept_one` 镜像，避免自相矛盾的历史）：
    //   * 已拒绝 → 幂等返回（不重复写原子）；
    //   * 已接受 → `precondition_failed`（已采纳的建议不能被"拒绝"）。
    let (accepted_by, rejected_by) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let mut accepted_by = None;
        let mut rejected_by = None;
        for atom in document.log().iter() {
            if atom.payload.get("target_atom_id").and_then(Value::as_str) != Some(suggestion_id) {
                continue;
            }
            match atom.kind {
                AtomKind::AcceptSuggestion => accepted_by = Some(atom.id.clone()),
                AtomKind::RejectSuggestion => rejected_by = Some(atom.id.clone()),
                _ => {}
            }
        }
        (accepted_by, rejected_by)
    };
    if let Some(reject_atom) = rejected_by {
        return Ok(json!({
            "suggestion_id": suggestion_id,
            "reason": reason,
            "rejected_annotations": Vec::<Value>::new(),
            "already_rejected": true,
            "reject_atom_id": reject_atom,
            "note": "该建议此前已被拒绝，本次不重复写入",
        }));
    }
    if let Some(accept_atom) = accepted_by {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "建议 {suggestion_id} 已被接受（原子 {accept_atom}），不能拒绝已接受的建议；如需撤销请用 revert"
            )),
        ));
    }
    let result = ctx.commit(
        AtomKind::RejectSuggestion,
        json!({"target_atom_id": suggestion_id, "reason": reason}),
    )?;
    // 把引用该建议的标注一并置为 rejected（12.6：记录原因 + 状态更新）。
    let (rejected_annotations, pending) = {
        let document = ctx.workspace.document_mut(&ctx.doc_id).map_err(|_| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        // 过滤条件没有 suggestion_id 字段，按状态取回后在内存里筛（待处理标注数量很小）。
        let filter = AnnotationFilter {
            status: Some(AnnotationStatus::Pending),
            suggestion_id: Some(suggestion_id.to_owned()),
            ..AnnotationFilter::default()
        };
        let ids: Vec<String> = document
            .annotations()
            .list(&filter)
            .into_iter()
            .map(|annotation| annotation.id)
            .collect();
        let mut rejected = Vec::new();
        for id in ids {
            rejected.push(document.annotations_mut().reject(&id, ctx.now)?);
        }
        (rejected, document.annotations().pending_count())
    };
    let _ = result;
    Ok(json!({
        "suggestion_id": suggestion_id,
        "reason": reason,
        "rejected_annotations": rejected_annotations,
        "pending_annotations": pending,
        "already_rejected": false,
    }))
}

/// 列出建议及其状态：状态由后续的 accept/reject 原子推导。
fn read_list_suggestions(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let filter = optional_str(args, "status");
    // AI 侧轮询：`since_seq` 取增量，`limit`/`offset` 用于分页。
    let since = optional_u64(args, "since_seq").unwrap_or(0);
    let limit = optional_u64(args, "limit").unwrap_or(50).clamp(1, 500) as usize;
    let offset = optional_u64(args, "offset").unwrap_or(0) as usize;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let log = document.log();
    // 先收集 accept/reject 的结果，再给建议打状态。
    let mut accepted: BTreeSet<String> = BTreeSet::new();
    let mut rejected: BTreeMap<String, String> = BTreeMap::new();
    for atom in log.iter() {
        match atom.kind {
            AtomKind::AcceptSuggestion => {
                if let Some(target) = atom.payload.get("target_atom_id").and_then(Value::as_str) {
                    accepted.insert(target.to_owned());
                }
            }
            AtomKind::RejectSuggestion => {
                if let Some(target) = atom.payload.get("target_atom_id").and_then(Value::as_str) {
                    let reason = atom
                        .payload
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    rejected.insert(target.to_owned(), reason);
                }
            }
            _ => {}
        }
    }
    let mut suggestions = Vec::new();
    for atom in log.iter() {
        if atom.kind != AtomKind::Suggest || atom.seq <= since {
            continue;
        }
        let status = if accepted.contains(&atom.id) {
            "accepted"
        } else if rejected.contains_key(&atom.id) {
            "rejected"
        } else {
            "pending"
        };
        if let Some(filter) = filter.as_deref() {
            if filter != status {
                continue;
            }
        }
        suggestions.push(json!({
            "suggestion_id": atom.id,
            "seq": atom.seq,
            "actor": atom.actor,
            "annotation_id": atom.payload.get("annotation_id").cloned().unwrap_or(Value::Null),
            "summary": atom.payload.get("summary").cloned().unwrap_or(Value::Null),
            "priority": atom.payload.get("priority").cloned().unwrap_or(json!(5)),
            "patch": atom.payload.get("patch").cloned().unwrap_or(Value::Null),
            "status": status,
            "reason": rejected.get(&atom.id).cloned().unwrap_or_default(),
        }));
    }
    // 排序：优先级降序，其次 seq 升序（同优先级按时间先后）—— 让审阅者先看要紧的建议。
    suggestions.sort_by(|left, right| {
        let left_priority = left["priority"].as_i64().unwrap_or(5);
        let right_priority = right["priority"].as_i64().unwrap_or(5);
        right_priority.cmp(&left_priority).then(
            left["seq"]
                .as_u64()
                .unwrap_or(0)
                .cmp(&right["seq"].as_u64().unwrap_or(0)),
        )
    });
    let total = suggestions.len();
    let pending = suggestions
        .iter()
        .filter(|suggestion| suggestion["status"] == json!("pending"))
        .count();
    // 冲突检测：比较各**待处理**建议的作用域（同图层/同对象/区域相交）。
    let pending_patches: Vec<(String, SuggestScope)> = suggestions
        .iter()
        .filter(|suggestion| suggestion["status"] == json!("pending"))
        .filter_map(|suggestion| {
            let id = suggestion["suggestion_id"].as_str()?.to_owned();
            let patch = suggestion["patch"].as_array()?;
            Some((id, suggestion_scope(patch)))
        })
        .collect();
    let mut conflicts = Vec::new();
    for (index, (left_id, left)) in pending_patches.iter().enumerate() {
        for (right_id, right) in pending_patches.iter().skip(index + 1) {
            if let Some(reason) = scopes_conflict(left, right) {
                conflicts.push(json!({
                    "suggestion_id": left_id,
                    "conflicts_with": right_id,
                    "reason": reason,
                }));
            }
        }
    }

    let page: Vec<Value> = suggestions.into_iter().skip(offset).take(limit).collect();
    Ok(json!({
        "suggestions": page,
        "conflicts": conflicts,
        "pending": pending,
        "total": total,
        "offset": offset,
        "limit": limit,
        "since_seq": since,
    }))
}

fn write_comment(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let text = require_str(args, "text")?;
    let mut payload = json!({"text": text});
    if let Some(target) = optional_str(args, "target_atom") {
        payload["target_atom"] = json!(target);
    }
    if let Some(object_id) = optional_str(args, "object_id") {
        payload["object_id"] = json!(object_id);
    }
    let result = ctx.commit(AtomKind::Comment, payload)?;
    finish_mutation(ctx, &result, None)
}

fn write_checkpoint(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let name = optional_str(args, "name")
        .unwrap_or_else(|| format!("ckpt_{}", yanshi_core::Ulid::new().encode()));
    let checkpoint_id = format!("ckpt_{}", yanshi_core::Ulid::new().encode());
    let anchor_seq = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        document.head_seq()
    };
    let mut payload = json!({
        "checkpoint_id": checkpoint_id,
        "name": name,
        "anchor_seq": anchor_seq,
    });
    if let Some(message) = optional_str(args, "message") {
        payload["message"] = json!(message);
    }
    let result = ctx.commit(AtomKind::CreateCheckpoint, payload)?;
    // 检查点关联快照由快照策略生成；这里同步尝试一次，保证时间旅行 O(1)（4.5/6.5）。
    let snapshot = ctx
        .workspace
        .document_mut(&ctx.doc_id)?
        .snapshot_now(ctx.now)
        .ok();
    let mut response = finish_mutation(ctx, &result, None)?;
    response["checkpoint_id"] = json!(checkpoint_id);
    response["snapshot_id"] = json!(snapshot);
    Ok(response)
}

fn write_declare_head(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let base_type = require_str(args, "base_type")?;
    let base_id = require_str(args, "base_id")?;
    let base = match base_type.as_str() {
        "atom" | "declare_head" => HeadBase::Atom(base_id.clone()),
        "checkpoint" => HeadBase::Checkpoint(base_id.clone()),
        other => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("base_type 必须是 atom 或 checkpoint，得到 {other}")),
            ))
        }
    };
    let reason = optional_str(args, "reason");
    let atom = ctx
        .workspace
        .document(&ctx.doc_id)
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?
        .declare_head_atom(&ctx.actor, &ctx.session, base, reason.as_deref());
    let result = ctx
        .workspace
        .commit(&ctx.doc_id, atom, &ctx.actor, ctx.owner)?;
    finish_mutation(ctx, &result, None)
}

fn write_revert_to(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let atom_id = require_str(args, "atom_id")?;
    let result = ctx.commit(
        AtomKind::DeclareHead,
        json!({"base": {"type": "atom", "id": atom_id}, "reason": "revert_to"}),
    )?;
    finish_mutation(ctx, &result, None)
}

fn write_restore_checkpoint(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let checkpoint_id = require_str(args, "checkpoint_id")?;
    let result = ctx.commit(
        AtomKind::DeclareHead,
        json!({"base": {"type": "checkpoint", "id": checkpoint_id}, "reason": "restore_checkpoint"}),
    )?;
    finish_mutation(ctx, &result, None)
}

fn write_batch(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let calls = require_object(args, "calls")?;
    let calls = calls.as_array().ok_or_else(|| missing("calls"))?;
    if calls.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("batch 至少需要一个调用"),
        ));
    }
    let registry = ToolRegistry::full();
    let changeset = Changeset::new_id();
    // batch 内的原子共用一个变更集（5.6：一个 batch 通常对应一个变更集）。
    let previous = ctx.changeset.replace(changeset.clone());
    let mut results = Vec::new();
    let mut atom_ids = Vec::new();
    for call in calls {
        let name = call
            .get("tool")
            .or_else(|| call.get("name"))
            .and_then(Value::as_str)
            .ok_or_else(|| missing("calls[].tool"))?;
        if name == "batch" {
            ctx.changeset = previous;
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("batch 不允许嵌套"),
            ));
        }
        let arguments = call
            .get("arguments")
            .or_else(|| call.get("args"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let value = match registry.get(name) {
            Some(spec) => match dispatch(spec, ctx, &arguments) {
                Ok(value) => ok_response(value),
                // 单个调用失败不回滚已提交的原子（append-only），逐项报告 5.7 错误。
                Err(error) => error_response(&error),
            },
            None => error_response(&YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("batch 中的工具 {name} 不存在")),
            )),
        };
        if let Some(atom_id) = value.get("atom_id").and_then(Value::as_str) {
            atom_ids.push(json!(atom_id));
        }
        results.push(json!({"tool": name, "result": value}));
    }
    ctx.changeset = previous;
    let head = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| document.head_seq())
        .unwrap_or(0);
    Ok(json!({
        "changeset_id": changeset,
        "calls": results,
        "count": results.len(),
        "atom_ids": atom_ids,
        "head": head,
    }))
}

// ---------------------------------------------------------------------------
// 标注工具
// ---------------------------------------------------------------------------

fn annotation_target(value: &Value) -> Result<AnnotationTarget> {
    let kind = value
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("region");
    match kind {
        "object" => {
            let object_id = value
                .get("object_id")
                .and_then(Value::as_str)
                .ok_or_else(|| missing("target.object_id"))?;
            Ok(AnnotationTarget::Object {
                object_id: object_id.to_owned(),
            })
        }
        _ => {
            let bbox = value
                .get("bbox")
                .map(parse_bbox)
                .transpose()?
                .or_else(|| Bbox::from_value(value))
                .ok_or_else(|| missing("target.bbox"))?;
            Ok(AnnotationTarget::Region { bbox })
        }
    }
}

fn write_create_annotation(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let annotation_type = AnnotationType::parse(&require_str(args, "type")?)?;
    let intent = AnnotationIntent::parse(&require_str(args, "intent")?)?;
    let target = annotation_target(require_object(args, "target")?)?;
    let content = optional_str(args, "content").unwrap_or_default();
    let head_seq = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| document.head_seq())
        .unwrap_or(0);
    let document = ctx.workspace.document_mut(&ctx.doc_id)?;
    let id = document.annotations_mut().create(
        &ctx.doc_id,
        &ctx.actor,
        head_seq,
        NewAnnotation {
            annotation_type,
            target,
            content,
            intent,
            suggestion_id: optional_str(args, "suggestion_id"),
        },
        ctx.now,
    );
    let pending = document.annotations().pending_count();
    document.broadcaster_mut().publish_annotations(pending);
    Ok(json!({"annotation_id": id, "pending": pending, "head_seq": head_seq}))
}

fn write_update_annotation(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let id = require_str(args, "annotation_id")?;
    let content = optional_str(args, "content");
    let intent = optional_str(args, "intent")
        .map(|text| AnnotationIntent::parse(&text))
        .transpose()?;
    // 设计 4.6 只列三态、未规定转换规则：这里只接受 `pending`（重开），
    // 其余状态转换必须走 resolve_annotation / reject_annotation，避免用 update 绕过状态机。
    let reopen = match optional_str(args, "status") {
        None => false,
        Some(status) if status == "pending" => true,
        Some(other) => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "update_annotation 的 status 只支持 pending（重开），得到 {other}；\
                     解决/拒绝请用 resolve_annotation / reject_annotation"
                )),
            ))
        }
    };
    let document = ctx.workspace.document_mut(&ctx.doc_id)?;
    let annotation = document
        .annotations_mut()
        .update(&id, content, intent, None, ctx.now)?;
    let annotation = if reopen {
        // 重开会清空 resolved_by/resolved_at。
        document.annotations_mut().reopen(&id, ctx.now)?
    } else {
        annotation
    };
    Ok(json!({"annotation": annotation, "pending": document.annotations().pending_count()}))
}

fn write_delete_annotation(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let id = require_str(args, "annotation_id")?;
    let document = ctx.workspace.document_mut(&ctx.doc_id)?;
    let annotation = document.annotations_mut().delete(&id, ctx.now)?;
    Ok(json!({"annotation": annotation}))
}

fn write_resolve_annotation(
    ctx: &mut ToolContext<'_>,
    args: &Value,
    status: AnnotationStatus,
) -> Result<Value> {
    let id = require_str(args, "annotation_id")?;
    let atom_id = optional_str(args, "atom_id");
    let document = ctx.workspace.document_mut(&ctx.doc_id)?;
    let annotation = match status {
        AnnotationStatus::Resolved => document.annotations_mut().resolve(&id, atom_id, ctx.now)?,
        _ => document.annotations_mut().reject(&id, ctx.now)?,
    };
    let pending = document.annotations().pending_count();
    Ok(json!({"annotation": annotation, "pending": pending}))
}

fn annotation_filter(args: &Value) -> Result<AnnotationFilter> {
    Ok(AnnotationFilter {
        status: optional_str(args, "status")
            .map(|text| AnnotationStatus::parse(&text))
            .transpose()?,
        actor: optional_str(args, "actor"),
        intent: optional_str(args, "intent")
            .map(|text| AnnotationIntent::parse(&text))
            .transpose()?,
        object_id: optional_str(args, "object_id"),
        suggestion_id: optional_str(args, "suggestion_id"),
    })
}

fn read_list_annotations(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let filter = annotation_filter(args)?;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let annotations = document.annotations().list(&filter);
    Ok(json!({
        "annotations": annotations,
        "count": annotations.len(),
        "pending": document.annotations().pending_count(),
    }))
}

fn read_get_annotation(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let id = require_str(args, "annotation_id")?;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let annotation = document.annotations().get(&id)?;
    Ok(json!({
        "annotation": annotation,
        "history": document.annotations().history(&id),
    }))
}

/// 参数对象必须是 JSON 对象（工具层统一入口校验）。
///
/// 这里做三件事（此前**只检查必填项是否存在** ✗，导致几类静默错误）：
/// 1. **拒绝未知参数**：拼错的名字（如 `param` 写成 `params`）不会被静默忽略，
///    而是报错并列出可用参数 —— 对 AI 与人都更有用；
/// 2. 必填项存在性；
/// 3. **声明类型校验**（`ParamKind` 此前只用于生成 MCP schema ✗）。
///
/// 可选参数显式传 `null` 视为"未提供"（客户端序列化常见）。
/// **框架级参数**：由调用方（HTTP/MCP）用于定位文档与会话，不属于任何单个工具的参数表，
/// 但会随参数体一起送达 —— 因此校验时必须放行。
///
/// 背景：MCP 层支持"工具参数里的 `doc_id` 优先"（`crates/yanshi-mcp/src/lib.rs`），
/// 若把未知参数一律拒绝，就会**破坏 MCP 调用** ✗（该风险由 `tool_validation_audit` 与
/// 既有的 MCP 测试共同暴露）。
const FRAMEWORK_PARAMS: [&str; 3] = ["doc_id", "actor", "session"];

/// 参数对象必须是 JSON 对象（工具层统一入口校验）。
///
/// 三件事（此前**只检查必填项是否存在**，导致几类静默错误）：
/// 1. **拒绝未知参数**（框架级参数除外）：拼错的名字不会被静默忽略，而是报错并列出可用参数；
/// 2. 必填项存在性；
/// 3. **声明类型校验**（`ParamKind` 此前只用于生成 MCP inputSchema）。
pub fn validate_args(spec: &ToolSpec, args: &Value) -> Result<()> {
    let object = args_object(args)?;
    // 1) 未知参数（框架级参数除外）。
    for key in object.keys() {
        if FRAMEWORK_PARAMS.contains(&key.as_str()) {
            continue;
        }
        if !spec.params.iter().any(|param| param.name == key) {
            let accepted: Vec<&str> = spec.params.iter().map(|param| param.name).collect();
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{} 不接受参数 {key}（拼写错误？）；可用参数：{}；框架级参数：{}",
                    spec.name,
                    if accepted.is_empty() {
                        "无".to_owned()
                    } else {
                        accepted.join(", ")
                    },
                    FRAMEWORK_PARAMS.join(", ")
                )),
            ));
        }
    }
    for param in spec.params {
        let Some(value) = object.get(param.name) else {
            if param.required {
                return Err(missing(param.name));
            }
            continue;
        };
        // 可选参数显式传 null：视为未提供。
        if value.is_null() && !param.required {
            continue;
        }
        let matches = match param.kind {
            ParamKind::String => value.is_string(),
            ParamKind::Integer => {
                value.is_i64()
                    || value.is_u64()
                    || value.as_f64().is_some_and(|number| number.fract() == 0.0)
            }
            ParamKind::Number => value.is_number(),
            ParamKind::Boolean => value.is_boolean(),
            ParamKind::Object => value.is_object(),
            ParamKind::Array => value.is_array(),
            ParamKind::Any => true,
        };
        if !matches {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "参数 {} 类型应为 {}，实际为 {}",
                    param.name,
                    param.kind.json_type(),
                    json_type_name(value)
                )),
            ));
        }
    }
    Ok(())
}

/// JSON 值的类型名（用于错误信息）。
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// 工具统计（10.2 的暴露分层效果）。
pub fn profile_summary(registry: &ToolRegistry) -> Value {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for tool in registry.tools() {
        *counts.entry(tool.profile.as_str().to_owned()).or_insert(0) += 1;
    }
    json!({
        "profiles": registry.profiles().iter().map(|profile| profile.as_str()).collect::<Vec<_>>(),
        "count": registry.len(),
        "by_profile": counts,
    })
}

fn object_type_name(object_type: ObjectType) -> &'static str {
    match object_type {
        ObjectType::Stroke => "stroke",
        ObjectType::Shape => "shape",
        ObjectType::Text => "text",
        ObjectType::Adjustment => "adjustment",
        ObjectType::Filter => "filter",
        ObjectType::RasterPatch => "raster_patch",
        ObjectType::Retouch => "retouch",
        ObjectType::Liquify => "liquify",
        ObjectType::Instance => "instance",
        ObjectType::Group => "group",
    }
}
