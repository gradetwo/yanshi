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

    /// **已实现**的工具组 ✓（**不含 `semantic`** ✓ —— 按既定裁定它只预留不开发 ✓）。
    ///
    /// **为什么要有这个清单** ✓：真实用户报告 ✓ —— `yanshi-mcp` 默认只开 `core` ✓
    /// ⇒ **后期调色、滤镜、蒙版、修复类工具全都"看不见"** ✗；
    /// 而 CLI 又没有简写 ✓ ⇒ 得手敲几十个字符的完整列表才能激活全部工具 ✗
    /// ⇒ `--profile all` 就是他给的解法 ✓。
    pub const IMPLEMENTED: [Profile; 8] = [
        Profile::Core,
        Profile::History,
        Profile::Changeset,
        Profile::Retouch,
        Profile::Conflict,
        Profile::Annotation,
        Profile::Collab,
        Profile::Structure,
    ];

    /// **解析逗号分隔的 profile 列表** ✓，支持 `all` 简写 ✓。
    ///
    /// **`all` = 全部"已实现"的组** ✓（**不含 `semantic`** ✗）——
    /// 与"缺省启用除 semantic 外的全部"是**同一条规矩** ✓ ⇒ 两处不会矛盾 ✓。
    /// 想单独开 semantic 就显式写 ✓（它在册 ✓，只是没有工具 ✓）。
    pub fn parse_list(text: &str) -> Result<Vec<Profile>> {
        let mut out = Vec::new();
        for name in text
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            if name == "all" {
                out.extend(Profile::IMPLEMENTED);
            } else {
                out.push(Profile::parse(name)?);
            }
        }
        if out.is_empty() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("--profile 不能为空 ⇒ 例如 core 或 all"),
            ));
        }
        Ok(out)
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
    /// **调用者的角色** ✓（Phase 5 ✓）—— **工具层强制**要用的就是它 ✓。
    ///
    /// **为什么必须带进上下文** ✓（本轮抓到的**第二个**授权漏洞 ✗）：
    /// 上一轮我把角色检查做在 **HTTP 入口**（`tool_call_with` ✓）⇒ 堵住了 `/api/tools/*` ✓，
    /// **却漏掉了 WebSocket** ✗ —— 那条路径直接调 `registry.call` ✓，
    /// 而且**无条件** `with_owner(true)` ✗ ⇒ **viewer 令牌可以经 WS 改文档** ✗。
    /// **实测复现** ✓（同一台服务器、同一个 viewer 令牌、同一个工具 ✓）：
    /// ```text
    /// HTTP      ⇒ {"ok":false,"error_code":"permission_denied"}   ✓ 正确拒绝 ✓
    /// WebSocket ⇒ {"ok":true,"layer_id":"L_viewer"}               ✗ 竟然成功 ✗
    /// ```
    /// **结论** ✓：权限检查**不能挂在某一个入口上** ✗ —— 入口会越加越多 ✓（HTTP / WS / MCP / batch 嵌套 ✓）
    /// ⇒ **必须在所有入口的必经之路**：`ToolRegistry::call` ✓。
    /// **缺省 `Owner`** ✓：本地进程（MCP ✓、测试 ✓、嵌入式 ✓）**本来就是 owner** ✓；
    /// 而**任何网络入口都必须显式设置它** ✗（HTTP/WS 现在都设了 ✓）—— 这条写在字段上 ✓，免得下次忘 ✓。
    pub role: crate::token::Role,
    /// 是否等待渲染完成（6.7 `wait_for_render`，默认 true）。
    pub wait_for_render: bool,
    /// **静默：不生成每次调用的预览** ✓（真实用户 §五-5 的"批量静默提交" ✓）。
    ///
    /// **为什么值得** ✓：用户原话 —— "批处理提交时为每个细分笔触**实时生成预览**与原子快照 ✓，
    /// 在大画布大规模排线时产生**额外的 CAS 临时 IO**" ✗。
    /// 一屏 500 笔 ⇒ **500 张预览** ✗ —— 而那些中间预览**没有人会看** ✗（批处理结束后画布就是最终状态 ✓）。
    /// **缺省 false** ✓：**不改变既有行为** ✓ —— 新开关一律缺省关闭 ✓，绝不偷偷改老调用方的观感与开销 ✗。
    pub silent: bool,
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
            // **缺省 owner** ✓：本地进程/测试就是 owner ✓；网络入口必须显式改 ✓（见字段说明 ✓）。
            role: crate::token::Role::Owner,
            wait_for_render: true,
            silent: false,
            wait_budget_ms: 500,
            now: yanshi_core::now_ms(),
            changeset: None,
        }
    }

    /// **设置调用者角色** ✓（网络入口必须调用 ✓，否则会以 owner 身份运行 ✗）。
    pub fn with_role(mut self, role: crate::token::Role) -> Self {
        self.role = role;
        self
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
        // **`begin_changeset` 打开的那个变更集，在这里自动生效** ✓ ——
        // 这是"调用方自己把多步操作归成一个变更集"的**唯一**收口点 ✓：
        // 普通笔 ✓、形状 ✓、填充 ✓、效果 ✓、图层 ✓……**所有**提交都经过这里 ✓
        // ⇒ 一处生效，全部并入 ✓（本项目反复吃过"只改一条路径"的亏 ✓）。
        // 显式的 `ctx.changeset`（内部批量用 ✓）优先 ✓ —— 那些调用自己知道该归到哪 ✓。
        if self.changeset.is_none() {
            if let Some(open) = self.workspace.open_changeset(&self.doc_id, &self.session) {
                self.changeset = Some(open);
            }
        }
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
        // **Phase 5：角色权限的落点就是这里** ✓（目标原文："**内核/工具层强制**" ✓）。
        //
        // **为什么从 HTTP 入口搬到这里** ✓（本轮抓到的**第二个**授权漏洞 ✗）：
        // 上一轮我把检查做在 `tool_call_with` ✓ ⇒ 只堵住了 `/api/tools/*` ✗，
        // 而 **WebSocket** 那条路径直接调本函数 ✓、还**无条件** `with_owner(true)` ✗
        // ⇒ **viewer 令牌可以经 WS 改文档** ✗。**实测复现** ✓（同一服务器/同一令牌/同一工具 ✓）：
        // `HTTP ⇒ permission_denied` ✓ 而 `WebSocket ⇒ ok:true, layer_id:L_viewer` ✗。
        // **结论** ✓：**权限检查不能挂在"某一个入口"上** ✗ —— 入口只会越加越多 ✓
        //（HTTP / WS / MCP / batch 嵌套 ✓）⇒ 必须挂在**所有入口的必经之路** ✓，也就是这里 ✓。
        //
        // **判据用工具自己声明的 `mutating`** ✓：哪些工具会改状态**只有一处定义** ✓
        // ⇒ 权限跟着它走 ✓，**不会漏** ✓、也**不会误伤只读工具** ✓。
        if spec.mutating && !ctx.role.can_edit() {
            return error_response(&YanshiError::new(
                ErrorCode::PermissionDenied,
                ErrorContext::detail(format!(
                    "该 token 的角色是 {}，不允许调用会改动文档的工具 {name}（读类工具不受限）",
                    ctx.role.as_str()
                )),
            ));
        }
        if let Err(error) = validate_args(spec, args) {
            return error_response(&error);
        }
        match dispatch(spec, ctx, args) {
            Ok(value) => ok_response(value),
            Err(error) => {
                // **事务：写操作失败 ⇒ 自动回滚已落的原子** ✓（这是"事务"与"变更集"的**唯一**区别 ✓，
                // 设计对两者都没写语义 ✓ ⇒ 取舍记在 `Workspace::transactions` 的说明里 ✓）。
                //
                // **只对 `mutating` 回滚** ✓：读操作失败也回滚的话 ✓，查一次东西就把人家的编辑撤了 ✗。
                // 放在这个收口 ✓ 是因为它同时知道 **spec（是否 mutating ✓）** 与 **结果（成功与否 ✓）** ✓，
                // 而且是**所有工具调用的必经之路** ✓ ⇒ 一处生效、全部受益 ✓。
                let mut response = error_response(&error);
                if spec.mutating {
                    if let Some(changeset) = ctx.workspace.transaction(&ctx.doc_id, &ctx.session) {
                        let outcome = revert_changeset_atoms(ctx, &changeset);
                        ctx.workspace.close_transaction(&ctx.doc_id, &ctx.session);
                        // **如实报告回滚结果** ✓（调用方必须知道"已经落下的部分被撤了" ✓，
                        // 否则它会以为只有这一步失败 ✗）。
                        let (reverted, ok) = match outcome {
                            Ok((count, _)) => (count, true),
                            Err(_) => (0, false),
                        };
                        response["rolled_back"] = json!({
                            "changeset_id": changeset,
                            "reverted": reverted,
                            "ok": ok,
                        });
                    }
                }
                response
            }
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
/// **把形状的 `geometry.bbox` 从数组归一化成 `{x,y,w,h}` 对象** ✓。
///
/// 渲染层（`yanshi-render/src/object.rs`）只读**对象**形式 ✗ ⇒ 数组形式若原样落库 ✓
/// 就会得到一个**零面积形状** ✗（存了、渲染空白 ✓）—— 与用户报的 P0 是同一类病 ✓。
/// 这里**在校验通过之后、提交之前**统一改写 ✓ ⇒ 下游只有一种形态 ✓，不必各自兼容 ✗。
fn normalize_shape_bbox(data: &mut Value) {
    // **扁平写法也要能画** ✓（真实用户第二次报告的原话 ✓）：
    // "矩形需要 `{kind:"rect", bbox:{x,y,w,h}}` 而非扁平的 `{kind:"rect", x,y,w,h}`，
    //  虽有错误提示引导，但初始学习成本较高" ✓。
    // **原则与本函数既有的"数组 bbox"一致** ✓：**既然一种自然写法通过了校验 ✓ 就必须真的能画** ✓ ——
    // 要么归一化它 ✓、要么明确拒绝它 ✗；**绝不能"接受但画不出来"** ✗（那正是当初 P0 的病根 ✓）。
    // 这里选择**归一化** ✓：`{x,y,w,h}` 写在 geometry 上是最顺手的写法 ✓，没有理由逼人多套一层 ✗。
    if let Some(geometry) = data.get_mut("geometry").and_then(Value::as_object_mut) {
        let flat = ["x", "y", "w", "h"]
            .iter()
            .all(|key| geometry.get(*key).and_then(Value::as_f64).is_some());
        if flat && geometry.get("bbox").is_none() {
            let number = |key: &str| geometry.get(key).and_then(Value::as_f64).unwrap_or(0.0);
            geometry.insert(
                "bbox".to_owned(),
                json!({"x": number("x"), "y": number("y"), "w": number("w"), "h": number("h")}),
            );
        }
    }
    let Some(geometry) = data.get_mut("geometry").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(values) = geometry.get("bbox").and_then(Value::as_array).cloned() else {
        return;
    };
    if values.len() < 4 {
        return;
    }
    let number = |index: usize| values.get(index).and_then(Value::as_f64).unwrap_or(0.0);
    geometry.insert(
        "bbox".to_owned(),
        json!({"x": number(0), "y": number(1), "w": number(2), "h": number(3)}),
    );
}

/// **校验形状几何** ✓ —— 拒绝解析不了的写法 ✓，而不是静默落一个空对象 ✗。
///
/// **依据是渲染层实际支持的三种** ✓（`yanshi-render/src/object.rs` ✓）：
/// `rect` / `ellipse` / `polygon`（`path` 也归多边形 ✓）。
/// 数据形态 ✓：`{kind:"rect"|"ellipse", bbox:{x,y,w,h}}` ✓ 或 `{kind:"polygon", points:[[x,y],…]}` ✓。
///
/// **为什么报错要写得具体** ✓：调用方（尤其 agent ✓）第一次猜错写法很正常 ✓，
/// 而"**从错误里学到正确写法**" ✓ 比"返回 ok 然后画出一片空白" ✓ 有用得多 ✓ ——
/// 后者会让人以为是自己画错了 ✗，前者一句话就解决了 ✓。
fn validate_shape_geometry(data: &Value) -> Result<()> {
    let geometry = data.get("geometry").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "形状缺少 geometry ✓：应形如 {\"kind\":\"rect\",\"bbox\":{\"x\":0,\"y\":0,\"w\":40,\"h\":30}}",
            ),
        )
    })?;
    let geometry = geometry.as_object().ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("geometry 必须是对象 ✓（含 kind 与其尺寸字段）"),
        )
    })?;
    let kind = geometry
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "geometry 缺少 kind ✓：可用 rect / ellipse / polygon（path 按多边形处理）",
                ),
            )
        })?;
    match kind {
        "rect" | "ellipse" => {
            let bbox = geometry.get("bbox").and_then(|bbox| {
                if let Some(values) = bbox.as_array() {
                    Some((
                        values.first()?.as_f64()?,
                        values.get(1)?.as_f64()?,
                        values.get(2)?.as_f64()?,
                        values.get(3)?.as_f64()?,
                    ))
                } else {
                    let object = bbox.as_object()?;
                    Some((
                        object.get("x")?.as_f64()?,
                        object.get("y")?.as_f64()?,
                        object.get("w")?.as_f64()?,
                        object.get("h")?.as_f64()?,
                    ))
                }
            });
            let Some((_, _, w, h)) = bbox else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{kind} 形状需要 geometry.bbox ✓，形如 {{\"x\":0,\"y\":0,\"w\":40,\"h\":30}} 或 [0,0,40,30]"
                    )),
                ));
            };
            if !(w > 0.0 && h > 0.0) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{kind} 形状的 bbox 宽高必须大于 0 ✓（实测 w={w}、h={h}）"
                    )),
                ));
            }
        }
        "polygon" | "path" => {
            let count = geometry
                .get("points")
                .and_then(Value::as_array)
                .map(|points| points.len())
                .unwrap_or(0);
            if count < 3 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{kind} 形状需要 geometry.points ✓（至少 3 个 [x,y] 点，实测 {count} 个）"
                    )),
                ));
            }
        }
        other => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "未知的形状 kind：{other} ✓（可用 rect / ellipse / polygon；path 按多边形处理）"
                )),
            ));
        }
    }
    Ok(())
}

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
    // **静默时连"取缓存地址"都跳过** ✓ —— 不只是省一次渲染 ✓：
    // 500 次调用各带一份预览 JSON ✓ 本身就是可观的响应体积与 IO ✓（用户报的正是这份额外开销 ✓）。
    if ctx.wait_for_render && !ctx.silent {
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
        name: "duplicate_layer",
        profile: Profile::Structure,
        summary: "复制图层（含其上的对象）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "要复制的图层"),
            param!("new_layer_id", String, false, "新图层 id；缺省自动生成"),
            param!("name", String, false, "新图层名；缺省为「原名 副本」"),
        ],
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
            // **加参数就必须同时登记** ✓ —— 这个错我犯过三次 ✗（`at`、`right_id`、`shape_id`、
            // `path_id` ✓），每次都是框架先报错、测试后失败 ✓。这次先写参数表 ✓。
            param!("medium", Object, false, "{id,version} 介质描述符：随这条原子一并记录（设计 11.1）"),
        ],
    },
    ToolSpec {
        name: "create_selection",
        profile: Profile::Structure,
        summary: "创建选区（形状 + 羽化 + 反选 + 组合模式）：约束其后的**笔触与擦除**落笔范围（覆盖度逐像素施加）",
        mutating: true,
        params: &[
            param!("selection_id", String, true, "选区 id"),
            param!("shape", Object, true, "几何 {kind:rect|ellipse|polygon, bbox, points?}"),
            param!("feather", Number, false, "羽化过渡宽度（像素，缺省 0）"),
            param!("invert", Boolean, false, "反选（缺省 false）"),
            param!("mode", String, false, "组合模式 new/add/subtract/intersect（缺省 new）"),
            param!("linked_layer", String, false, "仅约束该图层的落笔（缺省约束全文档）"),
        ],
    },
    ToolSpec {
        name: "delete_selection",
        profile: Profile::Structure,
        summary: "删除选区（tombstone）：删除后笔触/擦除不再受约束，且**已画内容按日志重算恢复原样**",
        mutating: true,
        params: &[param!("selection_id", String, true, "选区 id")],
    },
    ToolSpec {
        name: "list_selections",
        profile: Profile::Structure,
        summary: "列出选区（设计 4.4 的 Selection 模型：shape/feather/mode/invert/linked_layer）",
        mutating: false,
        params: &[],
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
                "{points,size,color,hardness,opacity,seed...}；\
                 points 支持 [[x,y]] 或 [[x,y,pressure]]（pressure 0..1，缺省 1.0）——\
                 按点给压力即可画出提按顿挫（内核早就支持，此前只是没写在这里）"
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
        summary: "绘制文本对象（内核用内置 5×7 ASCII 位图字体光栅化；CJK 字体子集属后续项）",
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
        name: "convert_to_shape",
        profile: Profile::Structure,
        summary: "把路径（或笔迹）转换成多边形形状（设计 792）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "要转换的路径或笔迹 id"),
            param!("shape_id", String, false, "转换后形状的 id；缺省自动生成"),
        ],
    },
    ToolSpec {
        name: "convert_to_path",
        profile: Profile::Structure,
        summary: "把笔迹转换成路径对象（设计 792）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "要转换的笔迹对象 id"),
            param!("path_id", String, false, "转换后路径的 id；缺省自动生成"),
        ],
    },
    ToolSpec {
        name: "transform_object",
        profile: Profile::Structure,
        summary: "旋转/缩放/平移一个对象（设计 783）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("rotate", Object, false, "{degrees} 旋转"),
            param!("scale", Object, false, "{x, y} 缩放"),
            param!("translate", Object, false, "{dx, dy} 平移"),
            param!("anchor", Object, false, "{x, y} 变换中心；缺省为对象包围盒中心"),
            param!("compose", Boolean, false, "是否叠加到现有变换上（缺省 true）"),
        ],
    },
    ToolSpec {
        name: "restore_object",
        profile: Profile::Structure,
        summary: "恢复已删除的对象（设计 783）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "get_object_history",
        profile: Profile::History,
        summary: "一个对象的原子版本链（设计 776）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "find_atom",
        profile: Profile::History,
        summary: "按条件检索原子（设计 776）",
        mutating: false,
        params: &[
            param!("kind", String, false, "原子种类"),
            param!("actor", String, false, "提交者"),
            param!("object_id", String, false, "涉及的对象"),
            param!("layer_id", String, false, "涉及的图层"),
            param!("since_seq", Integer, false, "起始序号（不含）"),
            param!("limit", Integer, false, "最多返回多少条"),
        ],
    },
    ToolSpec {
        name: "get_diff",
        profile: Profile::History,
        summary: "两个序号之间的日志差分（设计 776）",
        mutating: false,
        params: &[
            param!("from_seq", Integer, true, "起点序号（不含）"),
            param!("to_seq", Integer, false, "终点序号（含）；缺省到 HEAD"),
            param!("limit", Integer, false, "最多返回多少条原子"),
        ],
    },
    ToolSpec {
        name: "get_ancestors",
        profile: Profile::History,
        summary: "这个对象依赖谁（引用图的向上方向，设计 776）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "get_descendants",
        profile: Profile::History,
        summary: "谁依赖这个对象（引用图的向下方向，设计 776）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "submit_offline",
        profile: Profile::Changeset,
        summary: "重连时提交离线期间的原子；任一条不过校验则整批进悬空变更集（设计 12.4）",
        mutating: true,
        params: &[param!("atoms", Array, true, "[{kind, payload, actor?, session?}] 离线期间追加的原子")],
    },
    ToolSpec {
        name: "blob_gc",
        profile: Profile::History,
        summary: "Blob 三级生命周期：统计活跃/历史/孤儿；默认只报告，回收需 confirm",
        mutating: true,
        params: &[
            param!("dry_run", Boolean, false, "默认 true：只统计不删除"),
            param!("confirm", Boolean, false, "真删必须显式 true（删除不可逆）"),
            param!("ttl_days", Number, false, "孤儿 TTL，默认 7 天（设计 6.3）"),
            param!("demote", Boolean, false, "同时把「历史级」降冷到归档（设计 6.3 的后台迁移）"),
        ],
    },
    ToolSpec {
        name: "list_stashes",
        profile: Profile::Changeset,
        summary: "列出悬空变更集（UI 的“分支对比”用，设计 12.4）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "apply_stash",
        profile: Profile::Changeset,
        summary: "把悬空变更集强制应用到当前 HEAD（可能产生视觉错误，设计 12.4）",
        mutating: true,
        params: &[param!("stash_id", String, true, "悬空变更集 id")],
    },
    ToolSpec {
        name: "discard_stash",
        profile: Profile::Changeset,
        summary: "丢弃悬空变更集（不删 blob：Stash 的 blob 归历史级保留）",
        mutating: true,
        params: &[param!("stash_id", String, true, "悬空变更集 id")],
    },
    ToolSpec {
        name: "import_psd",
        profile: Profile::Core,
        summary: "只读导入 PSD 的合成图（不做图层结构，不写回 PSD）",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "落到哪个图层"),
            param!("blob_hash", String, true, "已上传的 PSD 字节（blob 先行）"),
            param!("object_id", String, false, "新对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "resample",
        profile: Profile::Retouch,
        summary: "重采样光栅对象的像素到新尺寸（非破坏：产出新 blob + supersede）",
        mutating: true,
        params: &[
            param!("object_id", String, true, "要重采样的光栅对象"),
            param!("width", Integer, false, "目标宽；与 height 一起给，或用 scale"),
            param!("height", Integer, false, "目标高"),
            param!("scale", Number, false, "缩放倍数（与 width/height 二选一）"),
            param!("filter", String, false, "nearest | bilinear（缺省 bilinear：介质是连续调）"),
        ],
    },
    ToolSpec {
        name: "resolve_conflict",
        profile: Profile::Conflict,
        summary: "解决采样性替换冲突（设计 12.3：组合宏，折叠器零改动）",
        mutating: true,
        params: &[
            param!("resolution", String, true, "keep_ours | keep_theirs | discard | merge"),
            param!("conflict_layer_id", String, false, "冲突图层 id；缺省自动找唯一的冲突图层"),
            param!("formal_layer_id", String, false, "keep_ours 时把我们的对象移回的正式图层"),
            param!("opponent_atom_id", String, false, "对方原子 id（keep_ours/discard 需要）"),
        ],
    },
    ToolSpec {
        name: "begin_transaction",
        profile: Profile::Changeset,
        summary: "开始一个事务：写操作失败时自动回滚已落的原子（设计 777）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "commit_transaction",
        profile: Profile::Changeset,
        summary: "收尾当前事务（原子保留，且可作为一组撤销）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "begin_changeset",
        profile: Profile::Changeset,
        summary: "开始一个变更集：此后本会话的提交都并入它（设计 793）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "commit_changeset",
        profile: Profile::Changeset,
        summary: "收尾当前变更集（原子保留）",
        mutating: false,
        params: &[],
    },
    ToolSpec {
        name: "abort_changeset",
        profile: Profile::Changeset,
        summary: "放弃当前变更集：把它里面的原子整体撤销",
        mutating: true,
        params: &[],
    },
    ToolSpec {
        name: "get_changesets",
        profile: Profile::Changeset,
        summary: "列出日志里的变更集（设计 793）",
        mutating: false,
        params: &[
            param!("limit", Integer, false, "最多返回多少个变更集"),
        ],
    },
    ToolSpec {
        name: "revert_changeset",
        profile: Profile::Changeset,
        summary: "整体撤销一个变更集（设计 793）",
        mutating: true,
        params: &[
            param!("changeset_id", String, true, "要撤销的变更集 id"),
        ],
    },
    ToolSpec {
        name: "path_edit",
        profile: Profile::Structure,
        summary: "笔迹路径编辑（设计 792；本片只实现 reverse/close/join）",
        mutating: true,
        params: &[
            param!("op", String, true, "reverse | close | join | merge | split"),
            param!("object_id", String, true, "目标对象 id（笔迹或路径）"),
            param!("other_id", String, false, "join / merge 的第二个对象"),
            param!("at", Integer, false, "split 的切口节点下标（仅开放路径）"),
            param!("right_id", String, false, "split 产生的右半 id；缺省自动生成"),
            // `convert_to_shape` 经 `path_edit` 调用时也用这两个参数 ✓
            //（上一轮踩过同一坑：加了算子却忘了登记参数 ✓ ⇒ 框架的参数校验直接拒绝 ✓）。
            param!("shape_id", String, false, "convert_to_shape 产生的形状 id；缺省自动生成"),
            // `convert_to_path` 经 `path_edit` 调用时用它 ✓。
            // **这是第三次犯同一个错** ✗（前两次是 `at`/`right_id` 与 `shape_id` ✓）：
            // 加算子/委托时**必须同时登记参数** ✓ —— 框架会在第一次调用时报出来 ✓，
            // 但那时已经是测试失败 ✓。以后加算子时**先写参数表** ✓。
            param!("path_id", String, false, "convert_to_path 产生的路径 id；缺省自动生成"),
            param!("mode", String, false, "boolean 的模式：union | intersect | subtract | xor"),
            param!("result_id", String, false, "boolean 结果 id 前缀；缺省自动生成"),
        ],
    },
    ToolSpec {
        name: "get_dependency_graph",
        profile: Profile::Core,
        summary: "查看对象依赖图（设计 9.4；544 的传播闭包）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "update_sync_policy",
        profile: Profile::Core,
        summary: "设置实例的同步策略（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("policy", String, true, "all 或 none"),
        ],
    },
    ToolSpec {
        name: "update_override",
        profile: Profile::Core,
        summary: "设置实例的覆盖变换（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("transform", Object, false, "{matrix,pivot} 覆盖变换；缺省清除覆盖"),
        ],
    },
    ToolSpec {
        name: "detach_instance",
        profile: Profile::Core,
        summary: "把实例脱离为独立对象（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("object_id", String, false, "脱离后新对象的 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "link_to_master",
        profile: Profile::Core,
        summary: "改写实例指向的 master（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("master_id", String, true, "新的 master 对象 id"),
        ],
    },
    ToolSpec {
        name: "get_resolved_state",
        profile: Profile::Core,
        summary: "查看实例解析到的 master 与包围盒（设计 9.2）",
        mutating: false,
        params: &[
            param!("object_id", String, true, "对象 id"),
        ],
    },
    ToolSpec {
        name: "create_instance",
        profile: Profile::Core,
        summary: "创建实例（设计 9.4）",
        mutating: true,
        params: &[
            param!("instance_id", String, true, "实例对象 id"),
            param!("layer_id", String, true, "所属图层"),
            param!("master_id", String, true, "master 对象 id"),
            param!("local_transform", Object, false, "{dx,dy} 或 {matrix,pivot}"),
        ],
    },
    ToolSpec {
        name: "create_group",
        profile: Profile::Core,
        summary: "创建对象组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("layer_id", String, true, "所属图层"),
            param!("members", Array, false, "成员对象 id 列表"),
        ],
    },
    ToolSpec {
        name: "add_to_group",
        profile: Profile::Core,
        summary: "把对象加入组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("object_id", String, true, "要加入的对象 id"),
        ],
    },
    ToolSpec {
        name: "remove_from_group",
        profile: Profile::Core,
        summary: "把对象移出组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("object_id", String, true, "要移出的对象 id"),
        ],
    },
    ToolSpec {
        name: "set_group_transform",
        profile: Profile::Core,
        summary: "平移整组（设计 9.4）",
        mutating: true,
        params: &[
            param!("group_id", String, true, "组对象 id"),
            param!("delta", Object, true, "{dx,dy} 平移增量"),
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
        name: "replace_object_data",
        profile: Profile::Core,
        summary: "替换对象的数据（设计 5.2 的 `supersede`）：文本可换文字、笔触可换点列/颜色等，对象保持可编辑",
        mutating: true,
        params: &[
            param!("object_id", String, true, "对象 id"),
            param!("data", Object, true, "新的对象数据（整体替换）"),
            param!("type", String, false, "可选：同时声明对象类型（缺省沿用原类型）"),
        ],
    },
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
        // 设计 6.3 的孤儿 GC。此前 `Document::collect_garbage` 已实现但**没有任何入口** ✗：
        // 渲染产生的 blob（预览/导出/逐像素自检）不属于任何原子引用，属于「孤儿」，
        // 只能靠 TTL 到期回收，而没有任何东西触发 GC。实测一个工作区因此累积到 1.3GB。
        //
        // 默认**干跑**（`confirm=false`）只报告，不删除任何东西；`confirm=true` 才真正回收。
        name: "collect_garbage",
        profile: Profile::Structure,
        summary: "Blob 孤儿回收（设计 6.3）：默认只报告，confirm=true 才删除",
        mutating: false,
        params: &[
            param!("confirm", Boolean, false, "true 才真正删除孤儿；缺省 false 只报告"),
            param!(
                "ttl_seconds",
                Integer,
                false,
                "覆盖孤儿 TTL（秒）。只影响「多久没被引用才算过期」；缺省用服务端配置（7 天）"
            ),
        ],
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
            param!(
                "silent",
                Boolean,
                false,
                "true = 不生成每次调用的预览（大批量排线时省下大量临时 IO；缺省 false，行为不变）"
            ),
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
        name: "new_document",
        profile: Profile::Core,
        // **描述必须与实现一致** ✓（真实用户报告 ✓）：它原来写着"新建（**或清空重建**）" ✗
        // ⇒ 而实现**明确拒绝**覆盖已存在的 id ✓ ⇒ **遵循这份描述的 Agent 会撞墙** ✗ ——
        // 契约与行为不符 ✓ 比功能缺失更坏 ✗（调用方会按描述写出**必然失败**的代码 ✓）。
        // **语义照实写清** ✓：新画布 = **新的 doc_id** ✓（文档 id 就是持久单元 ✓，这是设计 ✓）。
        summary: "新建一个空白文档（给定 doc_id / 宽高 / 背景色）；已存在的 doc_id 会被拒绝，不会清空",
        mutating: true,
        params: &[
            param!("doc_id", String, false, "文档 id（缺省用当前会话的文档）"),
            param!("width", Integer, true, "宽（像素）"),
            param!("height", Integer, true, "高（像素）"),
            param!("background", Object, false, "背景 {r,g,b,a}（0..255；缺省不透明白）"),
        ],
    },
    ToolSpec {
        name: "medium_stroke",
        profile: Profile::Core,
        summary: "用介质插件（油画/水彩/马克笔/铅笔/像素）画一笔：服务端原生调用插件，产出带 medium 描述符的补丁",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("medium", String, true, "介质 id：oil | watercolor | marker | pencil | pixel | example"),
            param!("points", Array, true, "笔迹采样点 [[x,y,pressure?], ...]（pressure 0..1）"),
            param!("size", Number, false, "笔尖大小（缺省 24）"),
            param!("color", Object, false, "笔尖色 {r,g,b,a}（0..255；缺省不透明黑）"),
            param!("load", Number, false, "载墨 0..1（缺省 1；越画越少）"),
            param!("wetness", Number, false, "湿度 0..1（缺省 0.4）"),
            param!(
                "texture",
                Number,
                false,
                "纹理强度 0..1（缺省 0 = 插件原本的笔痕；越大越平滑，油画大面积铺色时用得上）"
            ),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "export_project",
        profile: Profile::Core,
        summary: "把一份文档打成 .yanshi 工程包（未压缩 tar：原子日志 + 元数据 + 引用到的 blob + 当场渲染的预览）",
        mutating: false,
        params: &[
            param!("path", String, true, "输出文件路径（建议以 .yanshi 结尾；内容其实是未压缩 tar）"),
            param!("doc_id", String, false, "要导出的文档（缺省当前会话的文档）"),
        ],
    },
    ToolSpec {
        name: "import_project",
        profile: Profile::Core,
        // **与 `export_project` 配对** ✓（用户当初报的正是"没有导入导出工程" ✓；导出早就有了 ✓，导入一直没有 ✗）。
        summary: "导入 .yanshi 工程包（未压缩 tar）；**不会覆盖**已存在的 doc_id，blob 会按内容哈希核对",
        mutating: true,
        params: &[
            param!("path", String, true, "工程包路径（.yanshi）"),
            param!("doc_id", String, false, "导入成哪个文档（缺省用包内 meta.json 记的那个）"),
        ],
    },
    ToolSpec {
        name: "gradient_fill",
        profile: Profile::Core,
        // **针对用户报过的"大面积背景难处理"** ✓：平铺纹理是一条路 ✓，**渐变**是另一条 ✓ ——
        // 而且渐变**不带纹理噪声** ✓，做天空、底色、光照过渡比笔刷铺要稳得多 ✓。
        summary: "在图层或指定区域填渐变（linear 线性 / radial 径向）；双色、可给角度与中心",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("from", Object, true, "起点色 {r,g,b,a}（0..255）"),
            param!("to", Object, true, "终点色 {r,g,b,a}"),
            param!("kind", String, false, "linear（缺省）/ radial"),
            param!("angle", Number, false, "linear 的方向角（度；0 = 从左到右，90 = 从上到下）"),
            param!("center", Object, false, "radial 的中心 {x,y}（缺省取区域中心）"),
            param!("radius", Number, false, "radial 的半径（缺省取区域对角线的一半）"),
            param!("region", Object, false, "只填这块 {x,y,w,h}；不给就整层（按画布尺寸）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "texture_background",
        profile: Profile::Core,
        // **它改文档** ✓ ⇒ `mutating: true`（新建图层 + 落一层底图 ✓；`update_layer` 的强制检查会认它 ✓）。
        // **两种用法** ✓：不给 `region` ⇒ **整幅背景** ✓（自建一层并**沉到最底** ✓）；
        // 给了 `region` ⇒ **纹理补丁** ✓（只在那一块铺 ✓、**不重排图层** ✗、没给 layer_id 时新建一层放在**原位** ✓）。
        summary: "把 CC0 纹理铺成背景或补丁（tile/stretch/cover；给 region 就只铺那一块）",
        mutating: true,
        params: &[
            param!("texture", String, true, "纹理名（assets/textures 或工作区缓存里的 .png）"),
            param!("mode", String, false, "tile（缺省）/ stretch / cover"),
            param!("region", Object, false, "只铺这块区域 {x,y,w,h}；不给就铺满整幅（并沉到底当背景）"),
            param!("layer_id", String, false, "铺到哪个图层（缺省新建一个并沉到最底）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "list_palette_colors",
        profile: Profile::Core,
        summary: "读一个调色板的颜色（.gpl / .kpl / open-color 那种 .json）⇒ 供界面显示与取色",
        mutating: false,
        params: &[
            param!("palette", String, true, "调色板名（assets/palettes 或工作区缓存里的文件）"),
            param!("limit", Integer, false, "最多返回多少色（缺省 512；0 = 不限）"),
        ],
    },
    ToolSpec {
        name: "brush_stroke",
        profile: Profile::Core,
        // **Hokusai 引擎驱动的笔触** ✓（用户裁定：采纳 Hokusai ✓）。
        // **它和 `medium_stroke` 的关系** ✓：两条独立引擎 ✓ ——
        // 介质插件是我们自己的 wasm ABI ✓，Hokusai 读的是 libmypaint 的 `.myb` ✓
        // ⇒ 前者给"我们自己的介质" ✓，后者给"**196 支现成的 MyPaint 笔刷**" ✓。
        summary: "用 .myb 笔刷（Hokusai 引擎）画一笔：brush 给名字，points 给 [[x,y,pressure],…]",
        mutating: true,
        params: &[
            param!("layer_id", String, true, "目标图层"),
            param!("brush", String, true, "笔刷名（assets/brushes 或工作区缓存里的 .myb；可省 .myb）"),
            param!("points", Array, true, "[[x,y,pressure],…]，压力 0..1（可省，缺省 0.5）"),
            param!("size", Number, false, "覆盖笔刷自带半径（直径像素；不给就用 .myb 里的设置）"),
            param!("object_id", String, false, "对象 id（缺省自动生成）"),
        ],
    },
    ToolSpec {
        name: "list_assets",
        profile: Profile::Core,
        summary: "列出某类资产：brush（.myb 笔刷）/ texture（PNG 纹理）/ palette（调色板）",
        mutating: false,
        params: &[
            param!("kind", String, true, "brush / texture / palette"),
        ],
    },
    ToolSpec {
        name: "import_asset",
        profile: Profile::Core,
        // **不改文档 ⇒ mutating: false** ✓（它只往工作区缓存里写文件 ✓）。
        // 但**仍需落盘工作区** ✓ —— 纯内存模式会**明确拒绝** ✓（而不是假装成功 ✗）。
        summary: "导入资产到工作区缓存：笔刷 .myb / 纹理 .png / 调色板 .json .kpl .gpl .txt",
        mutating: false,
        params: &[
            param!("kind", String, true, "brush / texture / palette"),
            param!("name", String, true, "保存的文件名（扩展名必须与种类相符）"),
            param!("path", String, false, "服务器本地文件路径（MCP 常用）"),
            param!("blob", Object, false, "{blob_hash,size,mime_type}（Web 先把文件上传成 blob 再用）"),
            param!("overwrite", Boolean, false, "已存在时是否替换（缺省 false ⇒ 报冲突，不静默覆盖）"),
        ],
    },
    ToolSpec {
        name: "list_textures",
        profile: Profile::Core,
        summary: "列出可用纹理（内置 assets/textures + 工作区缓存 <root>/textures）",
        mutating: false,
        // **参数表就是契约** ✗：此前这里挂着一个 `dir` ✓，而实现**忽略**它 ✗
        // ⇒ 调用方传了也不会变 ✓ ⇒ 正是"**接受了却没用**" ✓ ⇒ 去掉它 ✓
        //（要按种类看资产用 `list_assets` ✓）。
        params: &[],
    },
    ToolSpec {
        name: "export_png",
        profile: Profile::Core,
        summary: "把整幅（或指定区域）渲染成 PNG 落盘：任意尺寸、不经 base64、不受 512px 限制",
        mutating: false,
        params: &[
            param!("path", String, true, "输出文件路径（含 .png）"),
            param!("region", Object, false, "只导出该区域 {x,y,w,h}；缺省整幅"),
            param!("width", Integer, false, "输出宽（与 height 一起给；不给就按原尺寸）"),
            param!("height", Integer, false, "输出高"),
            param!("max_edge", Integer, false, "限制最长边（按比例缩放；与 width/height 二选一）"),
            param!("filter", String, false, "nearest | bilinear（缺省 bilinear）"),
        ],
    },
    ToolSpec {
        name: "get_atom",
        profile: Profile::Core,
        summary: "读取一条原子的完整记录（含净荷）",
        mutating: false,
        params: &[param!("atom_id", String, true, "原子 id")],
    },
    ToolSpec {
        name: "list_comments",
        profile: Profile::Collab,
        summary: "列出评论（协作通道的可读一侧）",
        mutating: false,
        params: &[
            param!("limit", Integer, false, "最多返回条数（缺省 50，上限 500）"),
            param!("since_seq", Integer, false, "只返回 seq 大于该值的评论（增量轮询用）"),
            param!("actor", String, false, "按作者过滤"),
            param!("object_id", String, false, "按被评论的对象过滤"),
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
        "collect_garbage" => read_collect_garbage(ctx, args),
        "create_layer" => write_create_layer(ctx, args),
        "update_layer" => write_update_layer(ctx, args),
        "delete_layer" => write_delete_layer(ctx, args),
        "reorder_layers" => write_reorder_layers(ctx, args),
        "duplicate_layer" => write_duplicate_layer(ctx, args),
        "import_image" => write_import_image(ctx, args),
        "draw_stroke" => write_draw(ctx, args, AtomKind::DrawStroke),
        "draw_shape" => write_draw(ctx, args, AtomKind::DrawShape),
        "draw_text" => write_draw(ctx, args, AtomKind::DrawText),
        "fill" => write_draw(ctx, args, AtomKind::Fill),
        "erase" => write_draw(ctx, args, AtomKind::Erase),
        "update_object" => write_update_object(ctx, args),
        "replace_object_data" => write_replace_object_data(ctx, args),
        "update_stroke" => write_update_stroke(ctx, args),
        "create_instance" => write_create_instance(ctx, args),
        "convert_to_shape" => write_convert_to_shape(ctx, args),
        "convert_to_path" => write_convert_to_path(ctx, args),
        "transform_object" => write_transform_object(ctx, args),
        "restore_object" => write_restore_object(ctx, args),
        "get_object_history" => read_get_object_history(ctx, args),
        "find_atom" => read_find_atom(ctx, args),
        "get_diff" => read_get_diff(ctx, args),
        "get_ancestors" => read_get_ancestors(ctx, args),
        "get_descendants" => read_get_descendants(ctx, args),
        "submit_offline" => write_submit_offline(ctx, args),
        "blob_gc" => write_blob_gc(ctx, args),
        "list_stashes" => read_list_stashes(ctx, args),
        "apply_stash" => write_apply_stash(ctx, args),
        "discard_stash" => write_discard_stash(ctx, args),
        "import_psd" => write_import_psd(ctx, args),
        "resample" => write_resample(ctx, args),
        "resolve_conflict" => write_resolve_conflict(ctx, args),
        "begin_transaction" => write_begin_transaction(ctx, args),
        "commit_transaction" => write_commit_transaction(ctx, args),
        "begin_changeset" => write_begin_changeset(ctx, args),
        "commit_changeset" => write_commit_changeset(ctx, args),
        "abort_changeset" => write_abort_changeset(ctx, args),
        "get_changesets" => read_get_changesets(ctx, args),
        "revert_changeset" => write_revert_changeset(ctx, args),
        "path_edit" => write_path_edit(ctx, args),
        "get_dependency_graph" => read_get_dependency_graph(ctx, args),
        "update_sync_policy" => write_update_sync_policy(ctx, args),
        "update_override" => write_update_override(ctx, args),
        "detach_instance" => write_detach_instance(ctx, args),
        "link_to_master" => write_link_to_master(ctx, args),
        "get_resolved_state" => read_get_resolved_state(ctx, args),
        "create_group" => write_create_group(ctx, args),
        "add_to_group" => write_add_to_group(ctx, args),
        "remove_from_group" => write_remove_from_group(ctx, args),
        "set_group_transform" => write_set_group_transform(ctx, args),
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
        "create_selection" => write_create_selection(ctx, args),
        "delete_selection" => write_delete_selection(ctx, args),
        "list_selections" => read_list_selections(ctx),
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
        "new_document" => write_new_document(ctx, args),
        "medium_stroke" => write_medium_stroke(ctx, args),
        "export_project" => write_export_project(ctx, args),
        "list_textures" => write_list_textures(ctx, args),
        "list_assets" => write_list_assets(ctx, args),
        "brush_stroke" => write_brush_stroke(ctx, args),
        "list_palette_colors" => write_list_palette_colors(ctx, args),
        "texture_background" => write_texture_background(ctx, args),
        "gradient_fill" => write_gradient_fill(ctx, args),
        "import_project" => write_import_project(ctx, args),
        "import_asset" => write_import_asset(ctx, args),
        "export_png" => write_export_png(ctx, args),
        "get_atom" => read_get_atom(ctx, args),
        "list_comments" => read_list_comments(ctx, args),
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

/// 校验并规范化 `data.medium` ✓（设计 11.1 的插件介质描述符）。
///
/// 设计对插件有两条硬约束 ✓：
/// 1. **「插件 `id + version` 随原子记录」** ✓ —— 描述符就存在对象的 `data.medium` 里，
///    随原子进入日志 ✓，因此**可审计** ✓；
/// 2. **「升级不自动改变旧文档渲染」** ✓ —— 旧对象身上钉着当时那个 `version` ✓，
///    安装新版本插件**不会**改写历史对象的渲染 ✓（有测试守住这条不变量 ✓）。
///
/// 设计未规定描述符的确切字段名与校验规则 ✗，这里取最小形态 `{id, version}` ✓：
/// `id` 非空、只允许字母数字点下划线连字符 ✓（它会进入日志与插件查找 ✓），`version` 为 ≥1 的整数 ✓。
fn validate_medium(data: &Value) -> Result<()> {
    let Some(medium) = data.get("medium") else {
        return Ok(());
    };
    let object = medium.as_object().ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("medium 必须是对象，形如 {\"id\": \"oil\", \"version\": 1}"),
        )
    })?;
    let id = object.get("id").and_then(Value::as_str).unwrap_or_default();
    if id.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("medium.id 不能为空"),
        ));
    }
    if !id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
    {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "medium.id 只允许字母、数字、点、下划线与连字符，得到 {id}"
            )),
        ));
    }
    match object.get("version").and_then(Value::as_u64) {
        Some(version) if version >= 1 => {}
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("medium.version 必须是 ≥1 的整数"),
            ))
        }
    }
    Ok(())
}

/// 从对象 `data` 里取出介质描述符（供 `list_objects`/`get_object` 与查看器显示 ✓）。
fn medium_of(object: &yanshi_core::Object) -> Option<Value> {
    object.data.get("medium").cloned()
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
        // 介质描述符随原子记录 ✓（设计 11.1），在对象列表里可见 ✓。
        "medium": medium_of(object),
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

/// 孤儿 GC：默认只报告（干跑），`confirm: true` 才删除。
///
/// **安全保证**（设计 6.3）：根集 = 全日志引用闭包 ∪ 活跃 Manifest，因此
/// revert、时间旅行、Stash 引用的 blob 一定不会被回收；只有既无引用、又超过 TTL 的孤儿才会被删。
fn read_collect_garbage(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let confirm = args
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let now = ctx.now;
    // TTL 覆盖：渲染产生的孤儿（预览/导出/自检）往往刚刚写入，按缺省 7 天无法回收。
    // 缩小 TTL 会更激进地回收孤儿，但**根集永不受影响**（日志/Manifest 引用的 blob 不会被删）。
    let ttl = args
        .get("ttl_seconds")
        .and_then(Value::as_i64)
        .map(|value| value.max(0))
        .unwrap_or_else(|| ctx.workspace.settings().orphan_ttl_seconds);
    // **工作区级**：根集必须覆盖**所有文档**的引用闭包，否则会删掉别的文档引用的 blob。
    // **一次扫描**同时得到计划与（可选的）回收报告：分两次扫描会因中间状态变化而不一致。
    let (report, plan) = if confirm {
        let (report, plan) = ctx.workspace.collect_garbage_with_ttl(now, ttl)?;
        (Some(report), plan)
    } else {
        (None, ctx.workspace.plan_garbage_with_ttl(now, ttl)?)
    };

    let expiring_bytes: u64 = plan.expiring.iter().map(|entry| entry.size).sum();
    let orphan_bytes: u64 = plan.orphans.iter().map(|entry| entry.size).sum();
    let historical_bytes: u64 = plan.historical.iter().map(|entry| entry.size).sum();

    let mut response = json!({
        "dry_run": !confirm,
        "ttl_seconds": ttl,
        "scanned": plan.active.len() + plan.historical.len() + plan.orphans.len(),
        "active": plan.active.len(),
        "historical": plan.historical.len(),
        "historical_bytes": historical_bytes,
        "orphans": plan.orphans.len(),
        "orphan_bytes": orphan_bytes,
        "expiring": plan.expiring.len(),
        "expiring_bytes": expiring_bytes,
        "reclaimed_blobs": 0,
        "reclaimed_bytes": 0,
        "note": if confirm {
            "已回收超过 TTL 的孤儿；日志与 Manifest 引用的 blob 不会被删除"
        } else {
            "干跑：未删除任何 blob；确认回收请传 confirm=true"
        },
    });

    if let Some(report) = report {
        response["reclaimed_blobs"] = json!(report.deleted.len());
        response["reclaimed_bytes"] = json!(report.bytes_reclaimed);
        response["scanned"] = json!(report.scanned);
        response["retained_active"] = json!(report.retained_active);
        response["retained_historical"] = json!(report.retained_historical);
    }
    Ok(response)
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
        // **这一句此前是过时的** ✗（真实用户据它以为介质还没做 ✓）：六个介质**早已实现并发行** ✓
        //（`crates/yanshi-medium-*` ✓ + `assets/mediums/*.wasm` ✓ + 对象里记着 id+version ✓）。
        // **它为什么仍不在这个列表里** ✓：这里是**笔刷**（内核自带的通用光栅笔刷 ✓），
        // 而油画/水彩是**插件介质** ✓ —— 插件由**宿主**加载 ✓：浏览器端各自实例化 ✓，
        // 服务端/MCP 侧因六个插件**导出同名 C 符号** ✗ 而**无法链进同一个二进制** ✗
        //（详见 implementation-notes 的架构结论 ✓）。
        "note": "列出的是内核自带的光栅笔刷；油画/水彩/马克笔/铅笔/像素是**插件介质**（已发行，在 assets/mediums）——浏览器端可直接选用；MCP/服务端侧因六个插件导出同名 C 符号而无法链入同一二进制，另见实现笔记",
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
        // **缺省名要"唯一且好认"** ✗（真实用户报告 ✓）：
        // 此前缺省用 `layer_id` 本身 ✓ ⇒ 自动 id 形如 `layer_<ULID>` ✓ ⇒ 界面上要么一长串 ✓、
        // 要么（示例工程里）**好几层都叫 `layer`** ✗ ⇒ **用户根本分不清在做哪一层** ✓
        // ⇒ 他这次"黑色是最下面图层，可是还是显示"的困惑 ✓ 有一半就是**无法分辨图层** ✗ 造成的 ✓。
        // **改法** ✓：用 id 的**末四位**做短名 ✓ ⇒ 既**唯一** ✓ 又**短到能读** ✓（"图层 0005" ✓）。
        "name": optional_str(args, "name").unwrap_or_else(|| {
            let tail = layer_id.get(layer_id.len().saturating_sub(4)..).unwrap_or("");
            if tail.is_empty() {
                "图层".to_string()
            } else {
                format!("图层 {tail}")
            }
        }),
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
    let mut value = finish_mutation(ctx, &result, region)?;
    // **把 `layer_id` 直接回给调用方** ✓（真实用户报的 P1 ✓）：
    // 此前返回里**只有 `atom_id`** ✗ ⇒ 调用方必须**再调一次 `list_layers`** 才能翻出真实 id ✓
    //（形如 `layer_01M3…` ✓）⇒ **多一次往返** ✓，而且**写脚本的 agent 必踩** ✓。
    // 现在它随结果一起回来 ✓ ⇒ 一次往返就够 ✓。
    value["layer_id"] = json!(layer_id);
    Ok(value)
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
    // **`blend_mode` 的值必须校验** ✓（真实用户报告 ✓）：此前它只当"合法**键名**"放行 ✓
    // ⇒ **非法字符串照样写进不可变原子日志** ✗，而渲染层遇到不认识的值会**静默退化为 `normal`** ✗
    // ⇒ 调用方看到 `ok: true` ✓、画面却**什么也没发生** ✗ ——
    // 正是本项目最忌讳的"**接受了却没用**" ✓（与最初 `draw_shape` 的 P0 **同一类病** ✓）。
    //
    // **清单取自渲染层** ✓（`BlendMode::NAMES` ✓）—— 两边各写一份必然漂移 ✗。
    // **报错列出可用值** ✓（与"错误里带可用选项"同一规矩 ✓）。
    if let Some(value) = patch.get("blend_mode") {
        let name = value.as_str().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "blend_mode 必须是字符串 ⇒ 可用：{}",
                    yanshi_render::blend::BlendMode::NAMES.join(" / ")
                )),
            )
        })?;
        if yanshi_render::blend::BlendMode::parse(name).is_none() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "不支持的混合模式 {name} ⇒ 可用：{}",
                    yanshi_render::blend::BlendMode::NAMES.join(" / ")
                )),
            ));
        }
    }
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

/// **复制图层** ✓（用户点名的图层功能之一 ✓；后端此前**没有** ✗）。
///
/// **记录的选择** ✓：
/// * **忠实复制** ✓：名称（缺省加 " 副本" ✓）、`type`/`parent_id`/`blend_mode`/`opacity`/
///   `visible`/`locked` 全部照搬 ✓，对象连 `data`/`transform`/`z_index`/`metadata` 一起搬 ✓；
/// * **对象引用同一批 blob** ✓（不复制像素 ✓）—— 日志模型里这是**正确**的 ✓：
///   blob 是内容寻址的 ✓ ⇒ 两份对象共享一份数据 ✓；编辑只改对象与原子 ✓，不会回头改 blob ✓；
/// * **副本落在原图层正上方** ✓：`alive_layers()` 的 z 序是"越大越上" ✓，
///   而 `ReorderLayers` 按**列表下标**赋 z ✓ ⇒ 这里提交一份**完整顺序**（副本插在原图层后一位 ✓）
///   ⇒ 位置**确定** ✓。**这一步不能省** ✗：只给 `z_index + 1` 会和上方图层**撞号** ✓，
///   撞号后由 id 决定先后 ✓ ⇒ "副本有时在上面、有时在下面" ✗ —— 那是最难察觉的一类错 ✓。
/// * **一个变更集** ✓ ⇒ 一次可整体撤销 ✓。
///
/// **边界** ✓：锁定不影响"复制"（复制是**读**操作 ✓，与"改内容"不同 ✓）——
/// 副本会**保留**锁定状态 ✓（忠实复制 ✓），想编辑再解锁 ✓。
fn write_duplicate_layer(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let new_layer_id = args
        .get("new_layer_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("layer_{}", yanshi_core::Ulid::new().encode()));
    let (source, objects, order) = {
        let state = document_state(ctx)?;
        let Some(layer) = state.layers.get(layer_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("图层 {layer_id} 不存在")),
            ));
        };
        if layer.is_deleted() {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("图层 {layer_id} 已被删除")),
            ));
        }
        // 该图层上的**存活对象** ✓，按 (z_index, id) 升序 ✓（与渲染顺序一致 ✓）。
        let mut objects: Vec<(String, i64, Value, Value, Value, bool, bool)> = state
            .objects
            .values()
            .filter(|object| !object.is_deleted() && object.layer_id == layer_id)
            .map(|object| {
                (
                    object.id.clone(),
                    object.z_index,
                    object.data.clone(),
                    json!(object.transform.matrix.to_vec()),
                    json!(object.transform.pivot.to_vec()),
                    object.visible,
                    object.locked,
                )
            })
            .collect();
        // 按 `(z_index, id)` 升序 ✓ —— 与渲染顺序一致 ✓（clippy 建议 `sort_by_key` ✓ 采纳 ✓）。
        objects.sort_by_key(|object| (object.1, object.0.clone()));
        let order: Vec<String> = state.alive_layers().iter().map(|l| l.id.clone()).collect();
        (
            json!({
                "name": layer.name,
                "type": format!("{:?}", layer.layer_type).to_lowercase(),
                "parent_id": layer.parent_id,
                "blend_mode": layer.blend_mode,
                "opacity": layer.opacity,
                "visible": layer.visible,
                "locked": layer.locked,
                "z_index": layer.z_index,
            }),
            objects,
            order,
        )
    };
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "{} 副本",
                source["name"].as_str().unwrap_or(layer_id.as_str())
            )
        });
    // 新顺序：副本插在原图层**后一位** ✓（= 正上方 ✓）。
    let position = order.iter().position(|id| id == &layer_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("图层 {layer_id} 不在存活图层序列里")),
        )
    })? + 1;
    let mut target_order = order.clone();
    target_order.insert(position, new_layer_id.clone());
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let mut head = 0u64;
    let mut failure: Option<YanshiError> = None;
    let mut created_objects = 0usize;
    let mut layer_payload = source.clone();
    layer_payload["layer_id"] = json!(new_layer_id);
    layer_payload["name"] = json!(name);
    // **构造期先不加锁** ✓ —— 这一步是本轮补锁之后**测试当场逼出来**的 ✓：
    // 副本若一开始就带锁 ✓，新加的锁定校验会**拒绝往副本里加对象** ✗
    //（"不能在锁定的图层 L1b 上新建对象" ✓）⇒ 复制一个被锁的图层会失败 ✓。
    // 取舍 ✓：**先建成解锁的 ✓、把对象放进去 ✓、最后再补上锁** ✓
    // ⇒ 结果与"忠实复制"完全一致 ✓（副本最终带锁 ✓），也不需要在校验里开特例 ✓
    //（开特例是更差的做法 ✗：那会让"锁定"重新变成可以绕过的摆设 ✓）。
    let source_locked = layer_payload["locked"].as_bool().unwrap_or(false);
    layer_payload["locked"] = json!(false);
    match ctx.commit(AtomKind::CreateLayer, layer_payload) {
        Ok(result) => head = head.max(result.head_seq),
        Err(error) => failure = Some(error),
    }
    if failure.is_none() {
        for (_, z_index, data, matrix, pivot, visible, locked) in &objects {
            let object_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
            let payload = json!({
                "object_id": object_id,
                "layer_id": new_layer_id,
                "z_index": z_index,
                "visible": visible,
                "locked": locked,
                "transform": {"matrix": matrix, "pivot": pivot},
                "data": data,
            });
            match ctx.commit(AtomKind::CreateObject, payload) {
                Ok(result) => {
                    head = head.max(result.head_seq);
                    created_objects += 1;
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
    }
    // **最后补上锁** ✓（只有源图层是锁的才需要 ✓）。
    if failure.is_none() && source_locked {
        match ctx.commit(
            AtomKind::SetProperty,
            json!({"layer_id": new_layer_id, "key": "locked", "value": true}),
        ) {
            Ok(result) => head = head.max(result.head_seq),
            Err(error) => failure = Some(error),
        }
    }
    if failure.is_none() {
        match ctx.commit(AtomKind::ReorderLayers, json!({"order": target_order})) {
            Ok(result) => head = head.max(result.head_seq),
            Err(error) => failure = Some(error),
        }
    }
    ctx.changeset = previous;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(json!({
        "ok": true,
        "layer_id": new_layer_id,
        "from": layer_id,
        "name": name,
        "objects": created_objects,
        "z_index": position,
        "changeset_id": changeset,
        "head": head,
    }))
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
    // **介质描述符可以随这一条原子一起记下** ✓（设计 11.1 的硬要求："插件 id + version 随原子记录，
    // 升级不自动改变旧文档渲染" ✓）。
    //
    // **为什么加这个参数** ✓：此前查看器要提交**两条**原子 ✗ —— `import_image` 建对象 ✓、
    // 再用 `replace_object_data` 把 `{bitmap, region, width, height, medium}` 整体钉上去 ✓。
    // 实测（无内核实例、端到端）**每次原子提交都不是免费的** ✓ ⇒ 一条能表达清楚的事分成两条 ✗
    // 就是白花一笔钱 ✓。现在描述符随导入一起提交 ✓ ⇒ **一笔介质只产生一条原子** ✓。
    //
    // **为什么放在净荷顶层** ✓（而不是塞进一个 `data` 子对象 ✗）：折叠器 `object_data` 的规则是
    // "有 `data` 就用 `data` ✓，否则用整个净荷" ✓ ⇒ 顶层放 ✓ 则对象数据 = 净荷（含 `medium` ✓），
    // 与原来"导入 + 替换"之后的**渲染相关字段逐字一致** ✓（只多一个描述符键 ✓ ——
    // 那正是设计要求记下的东西 ✓）；而 `data.blob_hash` 若被挪进子对象 ✓ 会让
    // `all_blob_refs` 的净荷扫描**找不到 blob** ✗ ⇒ 校验与保留都会出错 ✗。
    let payload = match args.get("medium") {
        Some(medium) if !medium.is_null() => json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "type": "raster_patch",
            "bitmap": bitmap,
            "region": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
            "width": region.w as u64,
            "height": region.h as u64,
            "medium": medium,
        }),
        _ => json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "type": "raster_patch",
            "bitmap": bitmap,
            "region": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
            "width": region.w as u64,
            "height": region.h as u64,
        }),
    };
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
    let mut data = require_object(args, "data")?.clone();
    validate_colors(&data)?;
    // 插件介质描述符（设计 11.1）：合法则随原子记录 ✓，非法直接拒绝 ✓。
    validate_medium(&data)?;
    // **形状几何必须校验** ✓（P0 修复 ✓）：此前 `draw_shape` **静默接受**任何 `geometry` ✓
    // ⇒ 解析不了的写法也会 `ok: true` ✓、落一个**空几何对象** ✓ ⇒ 存了、渲染一片空白 ✓
    //（真实用户报的原话：「**画了个寂寞**」✓，四种常见写法全都这样 ✓）。
    // **位置** ✓：与 `validate_colors` / `validate_medium` 并列 ✓ ⇒ **一处生效、所有绘制入口都受管** ✓
    //（本项目一贯的"从结构上根除 ✓，不靠记性"✗）。
    if kind == AtomKind::DrawShape {
        // **顺序要紧：先归一化、再校验** ✓（真实用户第二次报告让我发现了这个顺序 bug ✗）。
        //
        // 原来校验在前 ✓ ⇒ 它**只认规范形式** ✗ ⇒ 我虽然在归一化里加了"扁平写法 ⇒ bbox" ✓，
        // 但**根本走不到那里** ✗ ⇒ 用户报的 `{kind:"rect", x,y,w,h}` 照样被拒 ✓
        //（报错还理直气壮地说"需要 geometry.bbox" ✓ —— **规则与实现对不上** ✗）。
        // **现在的规则只有一条** ✓：**校验的是"归一化之后"的形式** ✓ ⇒
        // 无论调用方用对象 ✓、数组 ✓ 还是扁平写法 ✓，**能通过的都一定能画** ✓。
        normalize_shape_bbox(&mut data);
        validate_shape_geometry(&data)?;
        // **归一化：`bbox` 数组 ⇒ `{x,y,w,h}` 对象** ✓。
        //
        // **为什么必须做** ✓（这是我自己刚写的测试抓到的**第二个静默空** ✗）：
        // 我原来的校验**接受**了 `bbox: [10,10,40,30]` ✓，而**渲染层只读对象形式** ✗
        // ⇒ 于是它又会**静默落一个零面积形状** ✗ —— 与用户报的 P0 **完全同一类病** ✓。
        // **选择"归一化"而不是"拒绝"** ✓：数组写法与扁平写法都是**自然的** ✓，
        // 既然它们能通过校验 ✓，就应当**真的能画** ✓；拒掉只是在惩罚调用方 ✗。
        //（归一化已经在上面、**校验之前**做完了 ✓ —— 顺序见那段注释 ✓。）
    }
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));

    // 填充：`data` 是 `{color, region}`，但渲染层的形状图元读的是 `bbox`
    // （`parse_shape` 的 `geometry.bbox` 或顶层 `bbox`）✓。而且 `ObjectType` 里**没有** Fill ✓，
    // fold 会按 `payload.type` 决定对象类型、缺省落到 **Stroke** ✗ —— 于是填充对象被当作没有
    // `points` 的笔触解析，**一点像素都不画**（用户报告 + API 实测：`ok: true` 但着色 0 ✗）。
    // 因此在工具层把它**规范化成一个矩形形状**并显式声明类型 ✓，内核语义保持不变 ✓。
    let mut declared_type: Option<&'static str> = None;
    if kind == AtomKind::Fill {
        let region = data
            .get("region")
            .cloned()
            .or_else(|| data.get("bbox").cloned());
        let region = match region {
            Some(value) => value,
            None => {
                // 缺省填充整幅画布（用户直觉："填充图层"）。
                let document = ctx.workspace.document_mut(&ctx.doc_id)?;
                let (width, height) = (document.state().width, document.state().height);
                json!({"x": 0, "y": 0, "w": width, "h": height})
            }
        };
        // 复用既有的 `parse_bbox`：它同时接受 `{x,y,w,h}` 与 `[x,y,w,h]`
        //（`Bbox::from_value` 只认前者）。
        let bbox = parse_bbox(&region).map_err(|_| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("fill 的 region 必须是 {x,y,w,h} 或 [x,y,w,h]"),
            )
        })?;
        let object = data.as_object_mut().expect("data 已是对象");
        object.remove("region");
        object.insert(
            "bbox".to_owned(),
            serde_json::to_value(bbox).unwrap_or(Value::Null),
        );
        declared_type = Some("shape");
    }

    let mut payload = json!({
        "object_id": object_id,
        "layer_id": layer_id,
        "data": data,
    });
    if let Some(declared) = declared_type {
        payload["type"] = json!(declared);
    }
    let result = ctx.commit(kind, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

const OBJECT_PATCH_KEYS: [&str; 6] = [
    "visible", "locked", "z_index", "layer_id", "metadata", "type",
];

/// 通用「替换对象数据」✓ —— 设计把"修改对象内容"交给 `supersede`（5.2 修改类 ✓），
/// 而 `update_object` 只覆盖 6 个属性键（visible/locked/z_index/layer_id/metadata/type ✓），
/// 不覆盖内容 ✗。此前想改文本内容只能自己构造原子 ✓（`text_stays_an_editable_object` 就是这么做的 ✓），
/// 这个工具把那条路补成正式入口 ✓，让查看器与 Agent 不必手写原子 ✓。
///
/// 设计 10.x 的工具清单**没有**列出它 ✓（属设计未规定 ✓），因此按"设计邻近、只做最小事"实现：
/// **只替换 `data`** ✓（可选同时声明 `type` ✓），不碰其它属性 ✓；对象类型不变时仍是同一个可编辑对象 ✓。
fn write_replace_object_data(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let data = require_object(args, "data")?.clone();
    if !data.is_object() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("data 必须是对象"),
        ));
    }
    let mut payload = json!({"object_id": object_id, "data": data});
    if let Some(declared) = optional_str(args, "type") {
        payload["type"] = json!(declared);
    }
    let result = ctx.commit(AtomKind::Supersede, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

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

/// 组对象的成员列表 ✓（不存在的组返回 `None` ✓，让调用方给出准确错误 ✓）。
fn group_members(ctx: &mut ToolContext<'_>, group_id: &str) -> Option<Vec<String>> {
    let state = document_state(ctx).ok()?;
    let object = state.objects.get(group_id)?;
    if object.object_type != yanshi_core::ObjectType::Group {
        return None;
    }
    Some(
        object
            .data
            .get("members")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    )
}

/// 校验组与成员 ✓（**本片边界**：成员必须存在 ✓、且**不能是组** ✗）。
///
/// 组嵌套需要设计 9.2 的派生解析（`resolve_object` ✓）与 9.3 的循环检测 ✓，
/// 属于下一步 ✓ —— 这里**显式拒绝** ✓ 而不是静默接受一个不会生效的嵌套 ✓。
fn check_group_members(ctx: &mut ToolContext<'_>, group_id: &str) -> Result<()> {
    let state = document_state(ctx)?;
    if state.objects.get(group_id).map(|object| object.object_type)
        != Some(yanshi_core::ObjectType::Group)
    {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("组 {group_id} 不存在（或不是对象组）")),
        ));
    }
    Ok(())
}

/// 创建实例 ✓（设计 9.4 ✓；`resolve_object` 见 `fold.rs` 与 `render.rs` ✓）。
///
/// **本片边界（明确拒绝 ✓）**：`override` 与 `sync_policy`（除缺省 `all` 外）需要设计 9.3 的
/// 缓存共享与依赖图传播 ✓，尚未实现 ✓ ⇒ 这里**报错**而不是默默忽略 ✓ ——
/// 静默接受一个不生效的姿势比明确不支持糟得多 ✓（上一轮组工具已经吃过一次这个教训 ✓）。
///
/// **循环引用在折叠层被挡住** ✓（`fold.rs` 的 `CreateObject` ✓）：
/// 手工写入日志的环、自引用、过深的链都会在**提交时**被拒 ✓，而不是等到渲染时无限递归 ✓。
/// **查看对象依赖图** ✓（设计 9.4 `get_dependency_graph` ✓；传播闭包见 544 ✓）。
///
/// 目前依赖边只有一种 ✓：**实例 → master** ✓（`master_ref` ✓）。
/// 因此这里回答两件事 ✓：**谁依赖我**（`dependents` ✓，即需要一起失效的对象 ✓）
/// 与**我依赖谁**（`references` ✓）。两者都是**传递闭包** ✓（设计 544 说的是"传播闭包"✓），
/// 带深度上限 ✓ 做纵深防御 ✓（成环已在折叠层被挡 ✓）。
fn read_get_dependency_graph(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let state = document_state(ctx)?;
    if !state.objects.contains_key(object_id.as_str()) {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        ));
    }
    // **谁依赖我** ✓（反向传递闭包 ✓）—— 改动我时，这些对象的区域也要失效 ✓。
    let dependents = yanshi_render::dirty::dependents_of(&state, &object_id);
    // **我依赖谁** ✓（正向 ✓）：顺着 `master_ref` 走 ✓。
    let mut references: Vec<String> = Vec::new();
    let mut cursor = object_id.clone();
    for _ in 0..64 {
        let next = state
            .objects
            .get(&cursor)
            .and_then(|object| object.data.get("master_ref"))
            .and_then(|master_ref| master_ref.get("object_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        match next {
            Some(master) => {
                references.push(master.clone());
                cursor = master;
            }
            None => break,
        }
    }
    Ok(json!({
        "ok": true,
        "object_id": object_id,
        "dependents": dependents,
        "references": references,
    }))
}

/// **设置实例的同步策略** ✓（设计 9.4 `update_sync_policy` ✓）。
///
/// 本片支持 `all`（跟随 ✓，缺省 ✓）与 `none`（不同步 ✓）——
/// **"不同步"的落地方式设计未规定 ⇒ 记录选择** ✓：由**折叠层**在改策略的那一刻把解析结果
/// 快照进实例自身 ✓（见 `fold.rs` ✓；放那里是因为快照必须**由日志决定** ✓，
/// 重放时要在同一个 seq 得到同一份快照 ✓）。
fn write_update_sync_policy(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let instance_id = require_str(args, "instance_id")?;
    let policy = require_str(args, "policy")?;
    if policy != "all" && policy != "none" {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "本片只支持 policy: all 或 none（收到 {policy}）—— partial 需要设计 9.3 的依赖图传播"
            )),
        ));
    }
    {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(instance_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("实例 {instance_id} 不存在")),
            ));
        };
        if object.object_type != yanshi_core::ObjectType::Instance {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{instance_id} 不是实例")),
            ));
        }
    }
    let result = ctx.commit(
        AtomKind::SetProperty,
        json!({"object_id": instance_id, "key": "sync_policy", "value": policy}),
    )?;
    Ok(json!({"ok": true, "instance_id": instance_id, "policy": policy, "head": result.head_seq}))
}

/// **设置实例的覆盖变换** ✓（设计 9.4 `update_override` ✓）。
///
/// **设计未规定 `override.transform` 与 `local_transform` 的相对次序 ⇒ 记录选择** ✓：
/// `local_transform` 属于**引用**（"取 master 的哪一部分" ✓），`override` 属于**这次引用上的修正** ✓
/// ⇒ override 放在**最外** ✓（改它只动这一个实例 ✓，不碰 master、也不碰链接本身 ✓）。
/// 缺省（不带 `transform` ✓）表示**清除覆盖** ✓。
fn write_update_override(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let instance_id = require_str(args, "instance_id")?;
    {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(instance_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("实例 {instance_id} 不存在")),
            ));
        };
        if object.object_type != yanshi_core::ObjectType::Instance {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{instance_id} 不是实例")),
            ));
        }
    }
    let value = match args.get("transform") {
        Some(transform) if !transform.is_null() => json!({"transform": transform}),
        // 清除覆盖 ✓：置空对象即可 ✓（渲染端取不到 `override.transform` ⇒ 不施加 ✓）。
        _ => json!(null),
    };
    let result = ctx.commit(
        AtomKind::SetProperty,
        json!({"object_id": instance_id, "key": "override", "value": value}),
    )?;
    Ok(json!({"ok": true, "instance_id": instance_id, "override": value, "head": result.head_seq}))
}

/// **脱离实例** ✓（设计 9.4 `detach_instance` ✓）：把它变成**独立对象** ✓ ——
/// 位置与外观保持不变 ✓，但此后**不再跟随 master** ✓。
///
/// 做法 ✓（设计未规定原子序列 ⇒ 记录选择 ✓）：**两步** ——
/// ① 用 master 的 `data` + 合成后的变换**新建一个普通对象** ✓；
/// ② 把原实例 **tombstone** ✓。
/// 这样"脱离"在日志里是**可追溯的两条原子** ✓，且渲染结果与脱离前**逐像素一致** ✓
///（同一份几何 ✓、同一个变换 ✓）；用 `revert` 撤掉第 ② 步即可回到实例状态 ✓。
fn write_detach_instance(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let instance_id = require_str(args, "instance_id")?;
    let new_id = args
        .get("object_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        // **默认 id 必须唯一** ✓ —— 上一版写成 `detached_<doc_id>` ✗ ⇒
        // 同一文档里**第二次脱离必然撞 id** ✗（"对象已存在"✓，且**脱离前那一步已经提交** ✓ ⇒
        // 用户看到的是"报错了但实例已经被删掉" ✓ —— 是我上一轮自己标出的缺陷 ✓）。
        // 这里改用与仓库其它处一致的 ULID ✓（`obj_<ulid>` 的写法见本文件多处 ✓）。
        .unwrap_or_else(|| format!("detached_{}", yanshi_core::Ulid::new().encode()));
    let (layer_id, master_data, transform) = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(instance_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("实例 {instance_id} 不存在")),
            ));
        };
        if object.object_type != yanshi_core::ObjectType::Instance {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{instance_id} 不是实例")),
            ));
        }
        let layer_id = object.layer_id.clone();
        // 解析到 master 的**数据**与**合成变换** ✓（与渲染同一函数 ✓ ⇒ 口径一致 ✓）。
        let master_id = object
            .data
            .get("master_ref")
            .and_then(|master_ref| master_ref.get("object_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("实例缺少 master_ref.object_id".to_owned()),
                )
            })?;
        let Some(master) = state.objects.get(master_id) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("master {master_id} 不存在，无法脱离")),
            ));
        };
        if master.is_deleted() {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("master {master_id} 已被删除，无法脱离")),
            ));
        }
        // 合成变换 = 实例自身 ∘ local_transform ∘ master 自身 ✓（与 `resolve_instance` 同序 ✓）。
        let local = object
            .data
            .get("master_ref")
            .and_then(|master_ref| master_ref.get("local_transform"))
            .map(transform_from_json)
            .unwrap_or(yanshi_core::Transform::IDENTITY);
        let composed = compose_transforms(
            &object.transform,
            &compose_transforms(&local, &master.transform),
        );
        (layer_id, master.data.clone(), composed)
    };
    // **两步归到一个变更集** ✓（设计 5.6：「一个 batch 通常对应一个变更集」✓；
    // §793：「`revert_changeset` 整体撤销一个变更集」✓）。
    //
    // 为什么需要 ✓：脱离天生是两步（建副本 ✓ + tombstone 原实例 ✓）⇒
    // 若只成功一半 ✓，用户会看到"副本已建、实例还在"的半成品 ✓。
    // 变更集本身**不提供自动回滚** ✗（设计没这么说 ✓，实现也是逐条提交 ✓），
    // 但它让这个半成品**一次就能整体撤销** ✓（`revert_changeset` ✓），
    // 而不是让用户去日志里找两条原子分别 undo ✓ —— 这是**可恢复性**的提升 ✓，如实说明其边界 ✓。
    let changeset = yanshi_core::Changeset::new_id();
    let previous_changeset = ctx.changeset.replace(changeset.clone());
    let created = ctx.commit(
        AtomKind::CreateObject,
        json!({
            "object_id": new_id,
            "layer_id": layer_id,
            "type": master_data.get("type").cloned().unwrap_or(json!("shape")),
            "data": master_data,
            "transform": {"matrix": transform.matrix, "pivot": transform.pivot},
        }),
    );
    let removed = created
        .as_ref()
        .ok()
        .map(|_| ctx.commit(AtomKind::Tombstone, json!({"object_id": instance_id})));
    ctx.changeset = previous_changeset;
    let created = created?;
    let removed = removed.expect("创建成功后才提交 tombstone")?;
    Ok(json!({
        "ok": true,
        "detached_id": new_id,
        "removed_instance": instance_id,
        "changeset_id": changeset,
        "head": removed.head_seq,
        "created_head": created.head_seq,
    }))
}

/// **改写实例指向的 master** ✓（设计 9.4 `link_to_master` ✓）。
///
/// **成环由折叠层挡住** ✓（见 `fold.rs` 的两条入口 ✓）—— 这里只做存在性与类型检查 ✓，
/// 让"是不是环"只有一个判定处 ✓（不会两处各判一套 ✓）。
fn write_link_to_master(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let instance_id = require_str(args, "instance_id")?;
    let master_id = require_str(args, "master_id")?;
    {
        let state = document_state(ctx)?;
        let Some(instance) = state.objects.get(instance_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("实例 {instance_id} 不存在")),
            ));
        };
        if instance.object_type != yanshi_core::ObjectType::Instance {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{instance_id} 不是实例")),
            ));
        }
        if !state.objects.contains_key(master_id.as_str()) {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("master {master_id} 不存在")),
            ));
        }
    }
    // 保留原有的 `local_transform` ✓（只换指向 ✓）。
    let local_transform = {
        let state = document_state(ctx)?;
        state
            .objects
            .get(instance_id.as_str())
            .and_then(|object| object.data.get("master_ref"))
            .and_then(|master_ref| master_ref.get("local_transform"))
            .cloned()
            .unwrap_or_else(
                || json!({"matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], "pivot": [0.0, 0.0]}),
            )
    };
    let result = ctx.commit(
        AtomKind::SetProperty,
        json!({
            "object_id": instance_id,
            "key": "master_ref",
            "value": {"object_id": master_id, "local_transform": local_transform},
        }),
    )?;
    Ok(
        json!({"ok": true, "instance_id": instance_id, "master_id": master_id, "head": result.head_seq}),
    )
}

/// **查看实例解析结果** ✓（设计 9.2 ✓）—— 让"解析"这件事**可观测** ✓，
/// 否则验证只能靠像素反推 ✓（能用但笨 ✓）。
fn read_get_resolved_state(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let (object_type, master_id, local_transform) = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        (
            object.object_type,
            object
                .data
                .get("master_ref")
                .and_then(|master_ref| master_ref.get("object_id"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            object
                .data
                .get("master_ref")
                .and_then(|master_ref| master_ref.get("local_transform"))
                .cloned(),
        )
    };
    let bbox = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        yanshi_render::object::object_bbox_in(&state, object)
    };
    Ok(json!({
        "ok": true,
        "object_id": object_id,
        "type": format!("{object_type:?}").to_lowercase(),
        "is_instance": object_type == yanshi_core::ObjectType::Instance,
        "master_id": master_id,
        "local_transform": local_transform,
        // **包围盒来自与渲染同一个函数** ✓ ⇒ 读到的就是画出来的 ✓。
        "bbox": bbox.map(|bbox| json!([bbox.x, bbox.y, bbox.w, bbox.h])),
    }))
}

/// `{matrix, pivot}` → [`Transform`] ✓（与 `yanshi-render` 的同名逻辑一致 ✓）。
fn transform_from_json(value: &Value) -> yanshi_core::Transform {
    let mut transform = yanshi_core::Transform::IDENTITY;
    if let Some(matrix) = value.get("matrix").and_then(Value::as_array) {
        if matrix.len() == 6 {
            for (slot, item) in transform.matrix.iter_mut().zip(matrix) {
                *slot = item.as_f64().unwrap_or(0.0);
            }
        }
    }
    if let Some(pivot) = value.get("pivot").and_then(Value::as_array) {
        if pivot.len() == 2 {
            transform.pivot = [
                pivot[0].as_f64().unwrap_or(0.0),
                pivot[1].as_f64().unwrap_or(0.0),
            ];
        }
    }
    transform
}

/// 仿射合成 ✓：`outer ∘ inner`（先用 inner 再用 outer ✓），`pivot` 先折进矩阵 ✓。
fn compose_transforms(
    outer: &yanshi_core::Transform,
    inner: &yanshi_core::Transform,
) -> yanshi_core::Transform {
    let flatten = |transform: &yanshi_core::Transform| -> [f64; 6] {
        let m = transform.matrix;
        let (px, py) = (transform.pivot[0], transform.pivot[1]);
        [
            m[0],
            m[1],
            m[2],
            m[3],
            m[4] + px - (m[0] * px + m[2] * py),
            m[5] + py - (m[1] * px + m[3] * py),
        ]
    };
    let a = flatten(outer);
    let b = flatten(inner);
    let mut matrix = [0.0f64; 6];
    matrix[0] = a[0] * b[0] + a[2] * b[1];
    matrix[1] = a[1] * b[0] + a[3] * b[1];
    matrix[2] = a[0] * b[2] + a[2] * b[3];
    matrix[3] = a[1] * b[2] + a[3] * b[3];
    matrix[4] = a[0] * b[4] + a[2] * b[5] + a[4];
    matrix[5] = a[1] * b[4] + a[3] * b[5] + a[5];
    yanshi_core::Transform {
        matrix,
        pivot: [0.0, 0.0],
    }
}

fn write_create_instance(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let instance_id = require_str(args, "instance_id")?;
    let layer_id = require_str(args, "layer_id")?;
    let master_id = require_str(args, "master_id")?;
    if let Some(override_value) = args.get("override") {
        if !override_value.is_null() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "本片不支持 instance.override（需要设计 9.3 的缓存与依赖图传播）".to_owned(),
                ),
            ));
        }
    }
    if let Some(policy) = args.get("sync_policy").and_then(Value::as_str) {
        if policy != "all" {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "本片只支持 sync_policy: all（收到 {policy}）—— 需要设计 9.3 的依赖图传播"
                )),
            ));
        }
    }
    // master 必须**已存在且不是实例之外的东西都行** ✓ —— 实例套实例是允许的 ✓（设计 9.2 会逐层解析 ✓），
    // 但**成环会被折叠层拒绝** ✓。这里只检查存在性 ✓，给出比"成环"更直白的错误 ✓。
    {
        let state = document_state(ctx)?;
        if !state.objects.contains_key(master_id.as_str()) {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("master {master_id} 不存在")),
            ));
        }
    }
    let local_transform = args
        .get("local_transform")
        .cloned()
        .unwrap_or_else(|| json!({"matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], "pivot": [0.0, 0.0]}));
    let result = ctx.commit(
        AtomKind::CreateObject,
        json!({
            "object_id": instance_id,
            "layer_id": layer_id,
            "type": "instance",
            "data": {
                "master_ref": {"object_id": master_id, "local_transform": local_transform},
                "sync_policy": "all",
            },
        }),
    )?;
    Ok(
        json!({"ok": true, "instance_id": instance_id, "master_id": master_id, "head": result.head_seq}),
    )
}

fn write_create_group(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let group_id = require_str(args, "group_id")?;
    let layer_id = require_str(args, "layer_id")?;
    let members: Vec<String> = args
        .get("members")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    {
        let state = document_state(ctx)?;
        for member in &members {
            let Some(object) = state.objects.get(member) else {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("组成员 {member} 不存在")),
                ));
            };
            if object.object_type == yanshi_core::ObjectType::Group {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "本片不支持组嵌套（{member} 也是组）—— 需要设计 9.2 的派生解析"
                    )),
                ));
            }
        }
    }
    let result = ctx.commit(
        AtomKind::CreateObject,
        json!({
            "object_id": group_id,
            "layer_id": layer_id,
            "type": "group",
            "data": {
                "members": members,
                "group_transform": {"matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], "pivot": [0.0, 0.0]},
            },
        }),
    )?;
    Ok(json!({"ok": true, "group_id": group_id, "members": members, "head": result.head_seq}))
}

fn write_add_to_group(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let group_id = require_str(args, "group_id")?;
    let object_id = require_str(args, "object_id")?;
    check_group_members(ctx, group_id.as_str())?;
    let mut members = group_members(ctx, group_id.as_str()).unwrap_or_default();
    {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        if object.object_type == yanshi_core::ObjectType::Group {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("本片不支持组嵌套（需要设计 9.2 的派生解析）".to_owned()),
            ));
        }
    }
    if !members.iter().any(|member| member == &object_id) {
        members.push(object_id.to_owned());
    }
    let result = ctx.commit(
        AtomKind::SetProperty,
        json!({"object_id": group_id, "key": "members", "value": members}),
    )?;
    Ok(json!({"ok": true, "group_id": group_id, "members": members, "head": result.head_seq}))
}

fn write_remove_from_group(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let group_id = require_str(args, "group_id")?;
    let object_id = require_str(args, "object_id")?;
    check_group_members(ctx, group_id.as_str())?;
    let mut members = group_members(ctx, group_id.as_str()).unwrap_or_default();
    members.retain(|member| member != &object_id);
    let result = ctx.commit(
        AtomKind::SetProperty,
        json!({"object_id": group_id, "key": "members", "value": members}),
    )?;
    Ok(json!({"ok": true, "group_id": group_id, "members": members, "head": result.head_seq}))
}

fn write_set_group_transform(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let group_id = require_str(args, "group_id")?;
    check_group_members(ctx, group_id.as_str())?;
    let delta = args.get("delta").cloned().unwrap_or(Value::Null);
    // **缺参必须报错，绝不用 0 兜底** ✓ —— 实测过一次代价 ✓：
    // 我第一版用 `unwrap_or(0.0)` ✗ ⇒ `{"matrix": [...]}` 被悄悄变成 `{dx: 0, dy: 0}` ✓
    // ⇒ 折叠层看到的是**合法平移** ✓ ⇒ 界面返回 `ok: true` 而**什么都没动** ✓
    //（测试当场抓到 ✓："非平移的组变换应被拒绝：… ok: true" ✓）。
    // "静默接受一个不会生效的姿势"比"明确不支持"糟得多 ✗。
    let (Some(dx), Some(dy)) = (
        delta.get("dx").and_then(Value::as_f64),
        delta.get("dy").and_then(Value::as_f64),
    ) else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "set_group_transform 需要 {delta: {dx, dy}}（本片只支持平移；一般仿射与嵌套组尚未实现）"
                    .to_owned(),
            ),
        ));
    };
    // 组自身的 `group_transform` 也**累积**同一个增量 ✓（便于读取与将来迁移到派生解析 ✓）：
    // 折叠层会把它作用到每个成员的 `transform` 上 ✓（见 `fold.rs` 的说明 ✓）。
    let result = ctx.commit(
        AtomKind::SetProperty,
        json!({"object_id": group_id, "key": "group_transform", "value": {"dx": dx, "dy": dy}}),
    )?;
    Ok(
        json!({"ok": true, "group_id": group_id, "delta": {"dx": dx, "dy": dy}, "head": result.head_seq}),
    )
}

/// 取路径节点的坐标 ✓（`nodes` 的每一项是 `{x, y, in, out}` ✓）。
/// 把方向向量归一化 ✓（零向量 ⇒ 返回零 ✓，调用方据此退化为直线 ✓）。
fn normalize_dir(dx: f64, dy: f64) -> (f64, f64) {
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 1e-12 {
        (0.0, 0.0)
    } else {
        (dx / length, dy / length)
    }
}

fn point_xy(point: &Value) -> (f64, f64) {
    if let Some(pair) = point.as_array() {
        return (
            pair.first().and_then(Value::as_f64).unwrap_or(0.0),
            pair.get(1).and_then(Value::as_f64).unwrap_or(0.0),
        );
    }
    (
        point.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        point.get("y").and_then(Value::as_f64).unwrap_or(0.0),
    )
}

fn node_xy(node: &Value) -> (f64, f64) {
    (
        node.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        node.get("y").and_then(Value::as_f64).unwrap_or(0.0),
    )
}

/// **在节点下标处切开一条路径** ✓（设计 792 的 `split` ✓）。
///
/// **设计只给了名字 ⇒ 记录选择** ✓：
/// * 只在**开放路径**上工作 ✓ —— 闭合环在**一个**节点处切不开 ✓（需要两刀 ✓），
///   而设计没说第二刀怎么给 ✓ ⇒ **明确报错** ✓，不擅自发明 ✗；
/// * 切口节点的控制柄**按归属分** ✓：左半末节点保留 `in`、`out` 归零 ✓；
///   右半首节点保留 `out`、`in` 归零 ✓ ⇒ **两半合起来与原曲线完全一致** ✓
///   （测试用"墨量之和不变"守住 ✓）；
/// * 原对象**保留左半** ✓（沿用原 id 与样式 ✓）、右半是**新对象** ✓；
///   两步归一个变更集 ✓ ⇒ 一次可整体撤销 ✓。
fn write_path_split(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let at = args.get("at").and_then(Value::as_u64).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("split 需要 at（切开处的节点下标）".to_owned()),
        )
    })? as usize;
    let right_id = args
        .get("right_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("path_{}", yanshi_core::Ulid::new().encode()));
    let (layer_id, data, nodes, closed) = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        if object.object_type != yanshi_core::ObjectType::Path {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{object_id} 不是路径（split 作用于路径的节点）")),
            ));
        }
        let nodes: Vec<Value> = object
            .data
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let closed = object
            .data
            .get("closed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        (object.layer_id.clone(), object.data.clone(), nodes, closed)
    };
    if closed {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "split 不支持闭合路径：闭合环在一个节点处切不开（需要两个切口），\
                 而设计未规定第二个切口如何给出 ⇒ 这里明确拒绝，不擅自发明"
                    .to_owned(),
            ),
        ));
    }
    if at == 0 || at + 1 >= nodes.len() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "at 必须是内部的节点下标（1..{}），两侧都要留节点",
                nodes.len().saturating_sub(2)
            )),
        ));
    }
    let mut left = nodes[..=at].to_vec();
    let mut right = nodes[at..].to_vec();
    // **控制柄按归属分** ✓（合起来仍是同一条曲线 ✓）。
    if let Some(map) = left[at].as_object_mut() {
        map.insert("out".to_owned(), json!([0.0, 0.0]));
    }
    if let Some(map) = right[0].as_object_mut() {
        map.insert("in".to_owned(), json!([0.0, 0.0]));
    }
    let mut left_data = data;
    left_data["nodes"] = json!(left);
    left_data["closed"] = json!(false);
    let mut right_data = left_data.clone();
    right_data["nodes"] = json!(right);
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let committed_left = ctx.commit(
        AtomKind::Supersede,
        json!({"object_id": object_id, "data": left_data}),
    );
    let committed_right = committed_left.as_ref().ok().map(|_| {
        ctx.commit(
            AtomKind::CreateObject,
            json!({"object_id": right_id, "layer_id": layer_id,
                   "type": "path", "data": right_data}),
        )
    });
    ctx.changeset = previous;
    let committed_left = committed_left?;
    let committed_right = committed_right.expect("左半成功后才建右半")?;
    Ok(json!({
        "ok": true,
        "op": "split",
        "object_id": object_id,
        "right_id": right_id,
        "at": at,
        "left_nodes": at + 1,
        "right_nodes": nodes.len() - at,
        "changeset_id": changeset,
        "head": committed_left.head_seq.max(committed_right.head_seq),
    }))
}

/// **多边形布尔** ✓（设计 792 的 `boolean` ✓）。
///
/// **设计只给了名字 ⇒ 记录选择** ✓（几何部分见 `yanshi_render::polygon` 的模块说明 ✓）：
/// * **模式由参数给出** ✓（`union`/`intersect`/`subtract`/`xor` ✓）—— 设计只写了一个 `boolean` ✓，
///   而四种运算显然都需要 ✓ ⇒ 与其发明四个工具名 ✓，不如一个工具加一个模式 ✓；
/// * **来源可以是路径、笔迹或既有多边形形状** ✓（都先取成顶点列 ✓；路径用**渲染端同一套铺平** ✓）；
/// * **结果落成形状对象** ✓（多边形 ✓）；若结果是**多个环** ✓（相离的两块 ✓、异或的两块 ✓）
///   就各建一个 ✓（id 依次加后缀 ✓）；
/// * **非破坏 + 一步撤销** ✓：新建结果 ✓ 并 tombstone 两个输入 ✓，全部归**一个变更集** ✓
///   ⇒ 不满意可以一次撤回 ✓。
/// * **退化输入明确报错** ✓（交点在顶点上 ✓、共线重叠 ✓、零面积 ✓）——
///   几何层**拒绝**而不是给一个看起来对的多边形 ✓，错误原文直接回给调用方 ✓。
fn write_path_boolean(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    use yanshi_render::polygon::{boolean, BooleanMode};
    let object_id = require_str(args, "object_id")?;
    let other_id = require_str(args, "other_id")?;
    let mode_name = require_str(args, "mode")?;
    let Some(mode) = BooleanMode::parse(&mode_name) else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "未知的布尔模式 {mode_name}（可用：union / intersect / subtract / xor）"
            )),
        ));
    };
    let result_prefix = args
        .get("result_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("bool_{}", yanshi_core::Ulid::new().encode()));
    // 取两条来源的顶点列 ✓（路径/笔迹/多边形形状 ✓）。
    let (layer_id, first, second) = {
        let state = document_state(ctx)?;
        let read = |id: &str| -> Result<Vec<(f64, f64)>> {
            let Some(object) = state.objects.get(id) else {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("对象 {id} 不存在")),
                ));
            };
            match object.object_type {
                yanshi_core::ObjectType::Path => {
                    let nodes = object
                        .data
                        .get("nodes")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let closed = object
                        .data
                        .get("closed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Ok(yanshi_render::object::flatten_path(&nodes, closed))
                }
                yanshi_core::ObjectType::Stroke => Ok(object
                    .data
                    .get("points")
                    .and_then(Value::as_array)
                    .map(|points| points.iter().map(point_xy).collect())
                    .unwrap_or_default()),
                yanshi_core::ObjectType::Shape => {
                    let kind = object
                        .data
                        .get("geometry")
                        .and_then(|geometry| geometry.get("kind"))
                        .and_then(Value::as_str)
                        .unwrap_or("rect");
                    if kind != "polygon" {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "{id} 是 {kind} 形状 —— 布尔只支持多边形（请先 convert_to_shape）"
                            )),
                        ));
                    }
                    Ok(object
                        .data
                        .get("geometry")
                        .and_then(|geometry| geometry.get("points"))
                        .and_then(Value::as_array)
                        .map(|points| points.iter().map(point_xy).collect())
                        .unwrap_or_default())
                }
                other => Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{id} 是 {other:?} —— 布尔只作用于路径、笔迹或多边形形状"
                    )),
                )),
            }
        };
        let first = read(&object_id)?;
        let second = read(&other_id)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            unreachable!("上面已确认存在")
        };
        (object.layer_id.clone(), first, second)
    };
    let rings = boolean(&first, &second, mode).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("布尔运算无法进行：{}", error.detail())),
        )
    })?;
    if rings.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "布尔结果为空集（{} 与 {} 的 {}）—— 空集没有可以落成对象的多边形",
                object_id,
                other_id,
                mode.as_str()
            )),
        ));
    }
    // 结果落成形状 ✓（多环 ⇒ 多个对象 ✓），并 tombstone 两个输入 ✓ —— 全部一个变更集 ✓。
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let style = {
        let state = document_state(ctx)?;
        state
            .objects
            .get(object_id.as_str())
            .map(|object| object.data.clone())
            .unwrap_or(Value::Null)
    };
    let mut created_ids: Vec<String> = Vec::new();
    let mut head = 0u64;
    let mut failure: Option<YanshiError> = None;
    for (index, ring) in rings.iter().enumerate() {
        let id = if index == 0 {
            result_prefix.clone()
        } else {
            format!("{result_prefix}_{}", index + 1)
        };
        let (min_x, min_y, max_x, max_y) = ring.iter().fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |(min_x, min_y, max_x, max_y), (x, y)| {
                (min_x.min(*x), min_y.min(*y), max_x.max(*x), max_y.max(*y))
            },
        );
        let mut data = style.clone();
        if let Some(map) = data.as_object_mut() {
            map.remove("nodes");
            map.remove("points");
            map.remove("closed");
        }
        data["geometry"] = json!({
            "kind": "polygon",
            "points": ring.iter().map(|(x, y)| json!([x, y])).collect::<Vec<_>>(),
            "bbox": {"x": min_x, "y": min_y, "w": max_x - min_x, "h": max_y - min_y},
        });
        data["bbox"] = json!({"x": min_x, "y": min_y, "w": max_x - min_x, "h": max_y - min_y});
        match ctx.commit(
            AtomKind::CreateObject,
            json!({"object_id": id, "layer_id": layer_id, "type": "shape", "data": data}),
        ) {
            Ok(result) => {
                head = head.max(result.head_seq);
                created_ids.push(id);
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    if failure.is_none() {
        for source in [object_id.clone(), other_id.clone()] {
            match ctx.commit(AtomKind::Tombstone, json!({"object_id": source})) {
                Ok(result) => head = head.max(result.head_seq),
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
    }
    ctx.changeset = previous;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(json!({
        "ok": true,
        "op": "boolean",
        "mode": mode.as_str(),
        "object_id": object_id,
        "other_id": other_id,
        "results": created_ids,
        "rings": rings.len(),
        "area": rings.iter().map(|ring| yanshi_render::polygon::area(ring)).sum::<f64>(),
        "changeset_id": changeset,
        "head": head,
    }))
}

/// **把路径（或笔迹）转换成多边形形状** ✓（设计 792 的 `convert_to_shape` ✓）。
///
/// **设计未规定语义 ⇒ 记录选择** ✓：
/// * **形状取多边形** ✓（`geometry.kind = "polygon"` ✓）—— 形状本来就有 `Polygon` 这一种 ✓，
///   而多边形**天然闭合** ✓（渲染时补回首点 ✓）⇒ 这是"路径 ⇒ 形状"最直白的读法 ✓；
/// * **铺平复用渲染端的同一函数** ✓（`yanshi_render::object::flatten_path` ✓）——
///   本项目吃过"包围盒一处一套、渲染一处一套"的亏 ✓ ⇒ 形状的顶点与渲染所见**必然一致** ✓；
/// * 同时接受**路径**与**笔迹** ✓（笔迹的点列本身就是折线 ✓）；
/// * 样式字段原样保留 ✓（多边形的填充色取 `color` ✓），`nodes`/`points` 换成了 `geometry` ✓；
/// * 两步原子 ✓（建形状 ✓ + tombstone 原对象 ✓）归**一个变更集** ✓ ⇒ 一次可整体撤销 ✓。
fn write_convert_to_shape(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let shape_id = args
        .get("shape_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("shape_{}", yanshi_core::Ulid::new().encode()));
    let (layer_id, data, object_type) = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        (
            object.layer_id.clone(),
            object.data.clone(),
            object.object_type,
        )
    };
    let points: Vec<(f64, f64)> = match object_type {
        yanshi_core::ObjectType::Path => {
            let nodes = data
                .get("nodes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let closed = data.get("closed").and_then(Value::as_bool).unwrap_or(false);
            yanshi_render::object::flatten_path(&nodes, closed)
        }
        yanshi_core::ObjectType::Stroke => data
            .get("points")
            .and_then(Value::as_array)
            .map(|points| points.iter().map(point_xy).collect())
            .unwrap_or_default(),
        other => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{object_id} 是 {other:?}（convert_to_shape 只接受路径或笔迹；形状无需再转）"
                )),
            ))
        }
    };
    if points.len() < 3 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "多边形至少需要三个顶点（当前 {} 个）—— 少于三个点转成形状没有面积可言",
                points.len()
            )),
        ));
    }
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let mut shape_data = data;
    if let Some(map) = shape_data.as_object_mut() {
        map.remove("nodes");
        map.remove("points");
        map.remove("closed");
    }
    shape_data["geometry"] = json!({
        "kind": "polygon",
        "points": points.iter().map(|(x, y)| json!([x, y])).collect::<Vec<_>>(),
        "bbox": {"x": min_x, "y": min_y, "w": max_x - min_x, "h": max_y - min_y},
    });
    // 形状的包围盒也写一份顶层 `bbox` ✓（渲染与命中测试都认它 ✓，与 `draw_shape` 的载荷一致 ✓）。
    shape_data["bbox"] = json!({"x": min_x, "y": min_y, "w": max_x - min_x, "h": max_y - min_y});
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let created = ctx.commit(
        AtomKind::CreateObject,
        json!({"object_id": shape_id, "layer_id": layer_id, "type": "shape", "data": shape_data}),
    );
    let removed = created
        .as_ref()
        .ok()
        .map(|_| ctx.commit(AtomKind::Tombstone, json!({"object_id": object_id})));
    ctx.changeset = previous;
    let created = created?;
    let removed = removed.expect("建好形状后才 tombstone 原对象")?;
    Ok(json!({
        "ok": true,
        "shape_id": shape_id,
        "from": object_id,
        "vertices": points.len(),
        "bbox": [min_x, min_y, max_x - min_x, max_y - min_y],
        "changeset_id": changeset,
        "head": created.head_seq.max(removed.head_seq),
    }))
}

/// **把笔迹转换成路径** ✓（设计 792 的 `convert_to_path` ✓）。
///
/// **设计未规定转换语义 ⇒ 记录选择** ✓：节点取笔迹的**原始采样点** ✓、控制柄**留空**
///（零柄 ⇒ 贝塞尔退化成直线 ✓）⇒ **画面逐像素不变** ✓ —— 这条由测试守住 ✓，
/// 它是"转换"最该有的性质 ✓（换个对象类型不该改变看到的东西 ✓）。
/// 笔触样式（`size`/`color`/`hardness`/`appearance` ✓）**原样搬到路径上** ✓，
/// 因为渲染端路径与笔迹**共用同一套字段** ✓（见 `parse_path` 的说明 ✓）。
///
/// 两步原子 ✓（建路径 ✓ + tombstone 原笔迹 ✓）归**一个变更集** ✓ ⇒ 一次可整体撤销 ✓
///（与 `detach_instance`、`restore_object` 同一处理 ✓）。
fn write_convert_to_path(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let path_id = args
        .get("path_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("path_{}", yanshi_core::Ulid::new().encode()));
    let (layer_id, data) = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        if object.object_type != yanshi_core::ObjectType::Stroke {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{object_id} 不是笔迹（本片的 convert_to_path 只从笔迹转换；路径转路径无需转换）"
                )),
            ));
        }
        (object.layer_id.clone(), object.data.clone())
    };
    let Some(points) = data.get("points").and_then(Value::as_array).cloned() else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{object_id} 没有 points")),
        ));
    };
    if points.len() < 2 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("路径至少需要两个节点".to_owned()),
        ));
    }
    let nodes: Vec<Value> = points
        .iter()
        .filter_map(|point| {
            let (x, y) = if let Some(pair) = point.as_array() {
                (pair.first()?.as_f64()?, pair.get(1)?.as_f64()?)
            } else {
                (point.get("x")?.as_f64()?, point.get("y")?.as_f64()?)
            };
            Some(json!({"x": x, "y": y, "in": [0.0, 0.0], "out": [0.0, 0.0]}))
        })
        .collect();
    let mut path_data = data;
    // 样式字段原样保留 ✓；把 `points` 换成 `nodes` + `closed` ✓（渲染端两者共用一套样式 ✓）。
    if let Some(map) = path_data.as_object_mut() {
        map.remove("points");
    }
    path_data["nodes"] = json!(nodes);
    path_data["closed"] = json!(false);
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let created = ctx.commit(
        AtomKind::CreateObject,
        json!({"object_id": path_id, "layer_id": layer_id, "type": "path", "data": path_data}),
    );
    let removed = created
        .as_ref()
        .ok()
        .map(|_| ctx.commit(AtomKind::Tombstone, json!({"object_id": object_id})));
    ctx.changeset = previous;
    let created = created?;
    let removed = removed.expect("建好路径后才 tombstone 原笔迹")?;
    Ok(json!({
        "ok": true,
        "path_id": path_id,
        "from": object_id,
        "nodes": nodes.len(),
        "changeset_id": changeset,
        "head": created.head_seq.max(removed.head_seq),
    }))
}

/// **旋转 / 缩放 / 平移一个对象** ✓（设计 783 的 `transform_object` ✓）。
///
/// 与 `move_object` 的分工 ✓（设计把两者并列 ✓，这里记录本仓库的取舍 ✓）：
/// `move_object` 是**移动**（平移 ✓，或直接给一个绝对矩阵 ✓）；
/// `transform_object` 面向**人类可读的几何操作** ✓ —— 旋转角度 ✓、缩放比例 ✓、平移量 ✓，
/// 并可指定**变换中心** `anchor` ✓（缺省取对象包围盒中心 ✓，这也是"转它自己"的直觉 ✓）。
///
/// 实现刻意复用既有机制 ✓：算出一个 2×3 矩阵 ✓，再以 **`Move {transform}`** 提交 ✓
///（折叠层在载荷带 `transform` 时**赋值** ✓ ⇒ 绝对变换 ✓）；`compose: true`（缺省 ✓）
/// 时先把新矩阵**叠加**到对象当前变换上 ✓ ⇒ 可以"再转 30°" ✓。
///
/// **为什么不做成新原子种类** ✓：`Move`/`Transform` 已经承载了绝对矩阵 ✓，
/// 渲染、包围盒、脏区、命中测试全都跟着它走 ✓ ⇒ 加新种类只会多一条要维护的路径 ✗。
fn write_transform_object(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let compose = optional_bool(args, "compose").unwrap_or(true);
    // 读当前包围盒（用于缺省锚点 ✓）与当前变换（用于叠加 ✓）。
    let (current, bbox) = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        (
            object.transform,
            yanshi_render::object::object_bbox_in(&state, object),
        )
    };
    let anchor = match args.get("anchor") {
        Some(anchor) if !anchor.is_null() => {
            let x = anchor.get("x").and_then(Value::as_f64);
            let y = anchor.get("y").and_then(Value::as_f64);
            match (x, y) {
                (Some(x), Some(y)) => (x, y),
                _ => {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("anchor 需要 {x, y}".to_owned()),
                    ))
                }
            }
        }
        // **缺省锚点 = 对象包围盒中心** ✓ —— "转它自己"是最小惊讶 ✓；
        // 解析不出包围盒（例如实例的 master 暂不可用 ✓）⇒ 明确报错 ✓，不悄悄用 (0,0) ✗。
        _ => match bbox {
            Some(bbox) => (bbox.x + bbox.w / 2.0, bbox.y + bbox.h / 2.0),
            None => {
                return Err(YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(
                        "对象当前没有可用的包围盒，无法推出缺省 anchor；请显式给出 anchor"
                            .to_owned(),
                    ),
                ))
            }
        },
    };
    // 三个操作里**恰好一个** ✓（同时给多个会让"叠加顺序"变成没说清的事 ✓）。
    //
    // **先数、再算** ✓ —— 我第一版把计数写进 `if let ... else if ...` 链里 ✗
    // ⇒ 只有第一个分支能进 ⇒ 计数**永远 ≤1** ⇒ 这条校验是**死代码** ✓
    //（测试当场抓到："给了两个 应被拒绝：… ok:true" ✓）。计数必须在**分支之前**完成 ✓。
    let requested = ["rotate", "scale", "translate"]
        .iter()
        .filter(|key| {
            args.get(**key)
                .map(|value| !value.is_null())
                .unwrap_or(false)
        })
        .count();
    let mut local = [1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0];
    if let Some(rotate) = args.get("rotate").filter(|value| !value.is_null()) {
        let degrees = rotate
            .get("degrees")
            .and_then(Value::as_f64)
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("rotate 需要 {degrees}".to_owned()),
                )
            })?;
        let radians = degrees.to_radians();
        let (sin, cos) = (radians.sin(), radians.cos());
        local = [cos, sin, -sin, cos, 0.0, 0.0];
    } else if let Some(scale) = args.get("scale").filter(|value| !value.is_null()) {
        let sx = scale.get("x").and_then(Value::as_f64).unwrap_or(1.0);
        let sy = scale.get("y").and_then(Value::as_f64).unwrap_or(sx);
        if sx.abs() < 1e-6 && sy.abs() < 1e-6 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("scale 不能两个方向都为 0（会把对象压成空）".to_owned()),
            ));
        }
        local = [sx, 0.0, 0.0, sy, 0.0, 0.0];
    } else if let Some(translate) = args.get("translate").filter(|value| !value.is_null()) {
        local = [
            1.0,
            0.0,
            0.0,
            1.0,
            translate.get("dx").and_then(Value::as_f64).unwrap_or(0.0),
            translate.get("dy").and_then(Value::as_f64).unwrap_or(0.0),
        ];
    }
    if requested != 1 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "请给出 rotate / scale / translate **其中之一**（同时给多个会让叠加顺序变成没说清的事）"
                    .to_owned(),
            ),
        ));
    }
    // 绕 anchor：`T(anchor) · op · T(-anchor)` ✓。
    let about_anchor = yanshi_core::Transform {
        matrix: [
            local[0],
            local[1],
            local[2],
            local[3],
            local[4] + anchor.0 - (local[0] * anchor.0 + local[2] * anchor.1),
            local[5] + anchor.1 - (local[1] * anchor.0 + local[3] * anchor.1),
        ],
        pivot: [0.0, 0.0],
    };
    let final_transform = if compose {
        compose_transforms(&about_anchor, &current)
    } else {
        about_anchor
    };
    let result = ctx.commit(
        AtomKind::Move,
        json!({"object_id": object_id,
               "transform": {"matrix": final_transform.matrix, "pivot": final_transform.pivot}}),
    )?;
    Ok(json!({
        "ok": true,
        "object_id": object_id,
        "anchor": [anchor.0, anchor.1],
        "composed": compose,
        "matrix": final_transform.matrix,
        "head": result.head_seq,
    }))
}

/// **恢复已删除的对象** ✓（设计 783 的 `restore_object` ✓）。
///
/// 做法 ✓：找到删掉它的那些 `tombstone` 原子 ✓，**逐条 `Revert`** ✓，并把这些 revert
/// 归入**一个变更集** ✓（与 `revert_changeset` 同一处理 ✓ ⇒ 这次恢复本身也能被一次撤销 ✓）。
///
/// **设计未规定"恢复"的语义 ⇒ 记录选择** ✓：走**历史**这条路 ✓（撤销删除 ✓），
/// 而不是"新建一个同 id 的对象" ✗ —— 后者会丢掉它的原子血脉 ✓，
/// 而本项目的一切都建立在"日志即真相"上 ✓。若对象**本来就没被删** ✓ 则明确报错 ✓。
///
/// **边界 ✓**：本片只恢复**对象** ✓（`layer` 的删除恢复尚未提供 ✓，需要时单独讨论 ✓）。
fn write_restore_object(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let (tombstones, state_present, alive) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let mut tombstones: Vec<String> = Vec::new();
        for atom in document.log().iter() {
            if atom.kind != AtomKind::Tombstone {
                continue;
            }
            if atom.payload.get("object_id").and_then(Value::as_str) == Some(object_id.as_str()) {
                tombstones.push(atom.id.clone());
            }
        }
        let state = document.state();
        let present = state.objects.contains_key(object_id.as_str());
        let alive = state
            .objects
            .get(object_id.as_str())
            .map(|object| !object.is_deleted())
            .unwrap_or(false);
        (tombstones, present, alive)
    };
    if !state_present {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!(
                "对象 {object_id} 从未存在过（日志里没有它的删除记录，也不在状态里）"
            )),
        ));
    }
    if tombstones.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("对象 {object_id} 没有被删除过，无需恢复")),
        ));
    }
    if alive {
        return Ok(
            json!({"ok": true, "object_id": object_id, "restored": false,
                         "note": "对象当前是活的（删除已被撤销过），无需恢复"}),
        );
    }
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let mut reverted: Vec<String> = Vec::new();
    let mut failure: Option<YanshiError> = None;
    for tombstone in &tombstones {
        match ctx.commit(AtomKind::Revert, json!({"target": tombstone})) {
            Ok(_) => reverted.push(tombstone.clone()),
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    ctx.changeset = previous;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(json!({
        "ok": true,
        "object_id": object_id,
        "restored": true,
        "reverted_tombstones": reverted,
        "changeset_id": changeset,
    }))
}

/// 原子涉及的 `object_id` / `layer_id` ✓（工具与历史检索都按这两个字段过滤 ✓）。
fn atom_object_id(atom: &yanshi_core::Atom) -> Option<&str> {
    atom.payload.get("object_id").and_then(Value::as_str)
}

fn atom_layer_id(atom: &yanshi_core::Atom) -> Option<&str> {
    atom.payload.get("layer_id").and_then(Value::as_str)
}

/// 一份"原子的摘要" ✓（历史类只读工具共用一个形状 ✓ ⇒ 不会各自长出不同字段 ✓）。
fn atom_summary(atom: &yanshi_core::Atom) -> Value {
    json!({
        "atom_id": atom.id,
        "seq": atom.seq,
        "kind": atom.kind,
        "actor": atom.actor,
        "session": atom.session,
        "changeset_id": atom.changeset_id,
        "timestamp": atom.timestamp,
        "object_id": atom_object_id(atom),
        "layer_id": atom_layer_id(atom),
        "heavy": atom.is_heavy(),
        "state_effect": atom.kind.is_state_effect(),
    })
}

/// **一个对象的原子版本链** ✓（设计 776 的 `get_object_history` ✓）。
///
/// **设计只列了名字 ⇒ 记录选择** ✓：这里报的是"这个对象**当前生效**的原子链" ✓
///（`state.objects[id].versions` ✓，由折叠层维护 ✓）—— 也就是"它是怎么变成现在这样的" ✓。
/// 因此它天然**不含**被 `supersede` 掉的中间版本 ✗（那些不在生效链上 ✓）；
/// 想看**全部**经过的原子请用 `find_atom {object_id}` ✓（那是日志检索 ✓，两者互补 ✓）。
///
/// 每条还给出 `reverted` ✓：扫描日志里指向它的最近一次 `Revert`/`Reapply` ✓。
/// **边界如实说明 ✓**：只做**一层**判定 ✓（与提交校验一致 ✓ —— 校验层目前不允许
/// revert/reapply 一个 revert 原子 ✓，见 `revert_changeset` 的说明 ✓）。
fn read_get_object_history(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let state = document.state().clone();
    let Some(object) = state.objects.get(object_id.as_str()) else {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        ));
    };
    let versions: Vec<String> = object.versions.clone();
    // 最近一次动作 ✓（一层 ✓）。
    let mut action: std::collections::BTreeMap<String, (u64, String)> =
        std::collections::BTreeMap::new();
    for atom in document.log().iter() {
        if atom.kind != AtomKind::Revert && atom.kind != AtomKind::Reapply {
            continue;
        }
        if let Some(target) = atom.payload.get("target").and_then(Value::as_str) {
            action.insert(target.to_owned(), (atom.seq, atom.kind.as_str().to_owned()));
        }
    }
    let by_id: std::collections::BTreeMap<String, Value> = document
        .log()
        .iter()
        .map(|atom| (atom.id.clone(), atom_summary(atom)))
        .collect();
    let chain: Vec<Value> = versions
        .iter()
        .filter_map(|atom_id| by_id.get(atom_id).cloned())
        .map(|mut summary| {
            let reverted = summary
                .get("atom_id")
                .and_then(Value::as_str)
                .and_then(|atom_id| action.get(atom_id))
                .map(|(_, kind)| kind == "revert")
                .unwrap_or(false);
            summary["reverted"] = json!(reverted);
            summary
        })
        .collect();
    Ok(json!({
        "ok": true,
        "object_id": object_id,
        "type": format!("{:?}", object.object_type).to_lowercase(),
        "deleted": object.is_deleted(),
        "versions": chain,
        "count": chain.len(),
        "head_seq": document.head_seq(),
    }))
}

/// **按条件检索原子** ✓（设计 776 的 `find_atom` ✓）。
///
/// 与 `get_log` 的差别 ✓：这里多了 **`object_id` / `layer_id`** 两个过滤 ✓，
/// 并且**报告匹配总数** ✓（`get_log` 只按 `limit` 返回 ✓，调用方无法知道还有多少 ✓）；
/// 这两点正是"找某个对象到底经历了什么"时最需要的 ✓。
fn read_find_atom(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let since = optional_u64(args, "since_seq").unwrap_or(0);
    let limit = optional_u64(args, "limit").unwrap_or(200).min(2000) as usize;
    let kind_filter = optional_str(args, "kind");
    let actor_filter = optional_str(args, "actor");
    let object_filter = optional_str(args, "object_id");
    let layer_filter = optional_str(args, "layer_id");
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let matched: Vec<&yanshi_core::Atom> = document
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
        .filter(|atom| {
            object_filter
                .as_deref()
                .is_none_or(|object_id| atom_object_id(atom) == Some(object_id))
        })
        .filter(|atom| {
            layer_filter
                .as_deref()
                .is_none_or(|layer_id| atom_layer_id(atom) == Some(layer_id))
        })
        .collect();
    let total = matched.len();
    let atoms: Vec<Value> = matched.into_iter().take(limit).map(atom_summary).collect();
    Ok(json!({
        "ok": true,
        "atoms": atoms,
        "returned": atoms.len(),
        "total_matched": total,
        "truncated": total > limit,
    }))
}

/// **两个序号之间的日志差分** ✓（设计 776 的 `get_diff` ✓）。
///
/// **设计只列了名字 ⇒ 记录选择** ✓：这里做的是**日志层**的差分 ✓ ——
/// `(from_seq, to_seq]` 区间内的原子 ✓、按种类汇总 ✓、以及涉及到的对象与图层集合 ✓。
/// 它**不**做像素/求值层差分 ✗（那会把"两个状态各渲染一遍"的成本塞进一个只读工具 ✓，
/// 而且渲染缓存与容差语义会牵进来 ✓）—— 需要像素对比请用 `render_region` 各渲染一次 ✓。
fn read_get_diff(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let from_seq = optional_u64(args, "from_seq").unwrap_or(0);
    let limit = optional_u64(args, "limit").unwrap_or(500).min(5000) as usize;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let head = document.head_seq();
    let to_seq = optional_u64(args, "to_seq").unwrap_or(head);
    if to_seq < from_seq {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("to_seq {to_seq} 小于 from_seq {from_seq}")),
        ));
    }
    let mut atoms: Vec<Value> = Vec::new();
    let mut by_kind: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    let mut objects: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut layers: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for atom in document.log().iter() {
        if atom.seq <= from_seq || atom.seq > to_seq {
            continue;
        }
        *by_kind.entry(atom.kind.as_str().to_owned()).or_insert(0) += 1;
        if let Some(object_id) = atom_object_id(atom) {
            objects.insert(object_id.to_owned());
        }
        if let Some(layer_id) = atom_layer_id(atom) {
            layers.insert(layer_id.to_owned());
        }
        if atoms.len() < limit {
            atoms.push(atom_summary(atom));
        }
    }
    Ok(json!({
        "ok": true,
        "from_seq": from_seq,
        "to_seq": to_seq,
        "head_seq": head,
        "atoms": atoms,
        "atom_count": atoms.len(),
        "by_kind": by_kind,
        "objects": objects.into_iter().collect::<Vec<_>>(),
        "layers": layers.into_iter().collect::<Vec<_>>(),
    }))
}

/// **引用图的向上方向** ✓（设计 776 的 `get_ancestors` ✓）。
///
/// **设计只列了名字 ⇒ 记录选择** ✓：历史组里这一对按**引用关系**解释 ✓ ——
/// `ancestors` = "我依赖谁" ✓（例如实例 → master → … ✓），
/// `descendants` = "谁依赖我" ✓（与 `get_dependency_graph` 的 `dependents` 同一套闭包 ✓）。
/// **为什么这样选** ✓：对象**自身的原子版本链**已经由 `get_object_history` 提供 ✓，
/// 两者不重复 ✓；而"依赖方向"在 `get_dependency_graph` 里已经算得很准 ✓，复用即可 ✓。
fn read_get_ancestors(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let state = document_state(ctx)?;
    if !state.objects.contains_key(object_id.as_str()) {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        ));
    }
    let mut chain: Vec<Value> = Vec::new();
    let mut cursor = object_id.clone();
    let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for _ in 0..64 {
        let master = state
            .objects
            .get(&cursor)
            .and_then(|object| object.data.get("master_ref"))
            .and_then(|master_ref| master_ref.get("object_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(master) = master else { break };
        if !visited.insert(master.clone()) {
            chain.push(json!({"object_id": master, "cycle": true}));
            break;
        }
        chain.push(json!({"object_id": master}));
        cursor = master;
    }
    Ok(json!({"ok": true, "object_id": object_id, "ancestors": chain, "count": chain.len()}))
}

fn read_get_descendants(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let state = document_state(ctx)?;
    if !state.objects.contains_key(object_id.as_str()) {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        ));
    }
    let descendants = yanshi_render::dirty::dependents_of(&state, &object_id);
    Ok(json!({
        "ok": true,
        "object_id": object_id,
        "descendants": descendants,
        "count": descendants.len(),
    }))
}

fn read_get_changesets(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let limit = optional_u64(args, "limit").unwrap_or(100).min(1000) as usize;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    // 按变更集聚合 ✓（保持日志顺序 ✓ ⇒ 同一个变更集里的原子序号是连续的 ✓，但不假设一定连续 ✓）。
    let mut order: Vec<String> = Vec::new();
    let mut grouped: std::collections::BTreeMap<String, Vec<(u64, String, String)>> =
        std::collections::BTreeMap::new();
    for atom in document.log().atoms() {
        let Some(changeset) = atom.changeset_id.clone() else {
            continue;
        };
        let entry = grouped.entry(changeset.clone()).or_default();
        if entry.is_empty() {
            order.push(changeset);
        }
        entry.push((atom.seq, atom.id.clone(), kind_label(&atom.kind)));
    }
    let changesets: Vec<Value> = order
        .iter()
        .rev()
        .take(limit)
        .map(|changeset| {
            let atoms = grouped.get(changeset).cloned().unwrap_or_default();
            json!({
                "changeset_id": changeset,
                "atoms": atoms.len(),
                "first_seq": atoms.first().map(|(seq, _, _)| *seq),
                "last_seq": atoms.last().map(|(seq, _, _)| *seq),
                "kinds": atoms.iter().map(|(_, _, kind)| kind.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({"ok": true, "changesets": changesets, "count": changesets.len()}))
}

/// **整体撤销一个变更集** ✓（设计 793 ✓）。
///
/// 做法 ✓：为变更集里的**每条内容原子**提交一条 `Revert {target}` ✓，
/// 并且这些 revert **自己也在一个变更集里** ✓（便于引用与续算 ✓）。
///
/// **边界如实说明 ✓（本轮实测出来的设计缺口 ✓）**：
/// 设计 793 写着 `revert(revert(x)) ≡ reapply(x)` ✓，折叠层里**也预留了**这条语义 ✓，
/// 但**提交校验**拒绝"revert/reapply 一个 revert 原子" ✗
///（实测：`不能 revert 协作/历史原子 …（revert）` ✓、`不能 reapply …（revert）` ✓）
/// ⇒ 已撤销的变更集**无法通过本工具再撤销回来** ✗。
/// 因此这里**跳过历史原子并如实报告** `skipped_history_atoms` ✓，
/// **不擅自**替用户决定放开哪一层 ✓（放开校验 ✓ 还是删掉折叠层的分支 ✗ 是**设计决策** ✓）。
fn write_revert_changeset(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let changeset_id = require_str(args, "changeset_id")?;
    let (targets, already_reverted) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        // **撤销一个 revert ⇒ 用 `reapply`** ✓ —— 这正是设计 793 的原话
        //（`revert(revert(x)) ≡ reapply(x)` ✓），也是测试当场教给我的 ✓：
        // 我第一版对**所有**原子都发 `Revert` ✗ ⇒ 折叠层拒绝"revert 一个 revert 原子" ✓
        //（"不能 revert 协作/历史原子 …（revert）" ✓）⇒ "撤销一次撤销"根本走不通 ✗。
        // 所以这里按**每条原子自身的种类**选动作 ✓：是 `Revert` 就发 `Reapply` ✓，否则发 `Revert` ✓。
        //
        // **边界如实说明 ✓**：只处理**一层**嵌套 ✓。更深的嵌套（revert 的 revert 的 revert ✓）
        // 会因为同样的拒绝而**明确报错** ✓，而不是悄悄做错 ✓ —— 需要更深的语义时再按设计讨论 ✓。
        // **只撤销"内容原子"，跳过历史原子本身** ✓ —— 这是我本轮实测出来的**设计缺口** ✓：
        // 设计 793 写着 `revert(revert(x)) ≡ reapply(x)` ✓，折叠层里**也预留了**这条语义
        //（"revert 自身被更晚的有效 revert 撤销时失效" ✓），
        // 但**提交校验**把"revert/reapply 一个 revert 原子"**全部拒绝** ✗
        //（实测两句：`不能 revert 协作/历史原子 …（revert）` ✓ 与 `不能 reapply …（revert）` ✓）。
        // ⇒ 两层不一致：折叠层准备接收的形态，校验层不允许产生 ✓。
        // 这属于**设计决策** ✓（放开校验 ✓ 还是删掉折叠层那个分支 ✗），
        // 本片**不擅自决定** ✓：**跳过并如实报告** ✓，让调用方知道"这几条没能撤销" ✓。
        let mut targets: Vec<(String, bool)> = Vec::new();
        let mut found: usize = 0;
        let skipped_history = 0usize;
        for atom in document.log().atoms() {
            if atom.changeset_id.as_deref() != Some(changeset_id.as_str()) {
                continue;
            }
            found += 1;
            // **历史原子也要撤销** ✓（用户已拍板放开校验 ✓，见 `log.rs` 里那段说明 ✓）：
            // 撤销一条 `Revert` ⇔ `Reapply` ✓（设计 793 的恒等式 ✓，折叠层的动作表实现的正是它 ✓）
            // ⇒ 变更集里的一切都能被撤 ✓，**"撤销本身也能被一次撤销"** 于是成立 ✓。
            targets.push((atom.id.clone(), false));
        }
        // **"不存在"与"只含历史原子"是两件事** ✓：前者要报错 ✓；
        // 后者应当**成功返回并如实报告** ✓（`reverted: 0` + `skipped_history_atoms: N` ✓）——
        // 我第一版把两者混成一个 `targets.is_empty()` ✗ ⇒ 明明存在、只是撤不动的变更集
        // 被报成"不存在" ✓，反而更误导 ✓。
        if found == 0 {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!(
                    "变更集 {changeset_id} 不存在（日志里没有属于它的原子）"
                )),
            ));
        }
        (targets, skipped_history)
    };
    // 到这里 `targets` 可能为空 ✓（该变更集只含历史原子 ✓）⇒ 那不是错误 ✓，见上面的说明 ✓。
    // **撤销逻辑与 `abort_changeset` 共用一份** ✓ —— 两个工具只差"changeset_id 从哪来" ✓；
    // "同一事实两处各写一遍"迟早分叉 ✗（本项目吃过这个亏 ✓）。
    let (reverted_count, revert_changeset_id) = revert_changeset_atoms(ctx, &changeset_id)?;
    Ok(json!({
        "ok": true,
        "changeset_id": changeset_id,
        "reverted": reverted_count,
        // **保持原响应形状** ✓：这里一直是"被撤销的 id 列表" ✓（别因为换了实现就改形状 ✗）。
        "targets": targets.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
        "revert_changeset_id": revert_changeset_id,
        // **如实报告**跳过了多少条历史原子 ✓（名字要说清"跳过的是什么" ✓）。
        "skipped_history_atoms": already_reverted,
    }))
}

/// **笔迹路径编辑** ✓（设计 792 行：`path_edit`（split, merge, join, close, reverse, boolean,
/// convert_to_shape, convert_to_path ✓））。
///
/// **本片只做三个算子** ✓，而且理由要说清楚 ✓：设计把 `path_edit` 列在工具表里 ✓，
/// 但**没有规定"路径对象"在内核里怎么表达** ✗（`ObjectType` 里没有 Path ✓，
/// `Region` 里的 `path` 是另一回事 ✓）。`split`/`merge`/`boolean`/`convert_to_*`
/// 都需要那套模型 ✓ ⇒ 它们是**设计决策** ✓，本片**显式拒绝并说明** ✓，
/// **不擅自发明** ✗（"静默接受一个不会生效的姿势"是本项目反复吃亏的地方 ✓）。
///
/// 能做且语义明确的是这三个 ✓ —— 它们都落在**既有**的笔迹几何上 ✓（`data.points` ✓）：
/// * `reverse` ✓：反转点序 ✓。这条**不只是换个顺序** ✓：配合 `appearance` 的
///   `paint_load`（墨沿笔迹耗尽 ✓）与 `mixing`（笔尖取下方已有色 ✓），
///   反向之后**渲染结果会变** ✓ ⇒ 是一条可验证的性质 ✓；
/// * `close` ✓：把首点追加到末尾 ✓（已闭合则**幂等** ✓）；
/// * `join` ✓：把第二条笔迹的点接到第一条之后 ✓，并 tombstone 第二条 ✓（两步 ⇒ **一个变更集** ✓，
///   与 `detach_instance` 同一处理 ✓）。
///   **设计未规定合并后的画笔/外观归属 ⇒ 记录选择** ✓：保留**第一条**的
///   `size`/`color`/`hardness`/`appearance` ✓（"接进第一条"是最小惊讶 ✓）。
///
/// **开始一个变更集** ✓（设计 793 ✓）。
///
/// **设计未规定 id 从哪来 ⇒ 记录选择** ✓：**由服务端生成并返回** ✓。
/// 让调用方指定 id 需要一套校验与冲突规则 ✓，而设计没说 ✗ ⇒ 不擅自发明 ✗。
///
/// **设计未规定重复 begin 怎么办 ⇒ 记录选择** ✓：**报错** ✓ —— 静默复用会让调用方以为
/// "新开了一个" ✓、静默新建会让前一个永远挂在打开状态 ✗，报错最诚实 ✓。
fn write_begin_changeset(ctx: &mut ToolContext<'_>, _args: &Value) -> Result<Value> {
    match ctx.workspace.begin_changeset(&ctx.doc_id, &ctx.session) {
        Some(changeset) => Ok(json!({"ok": true, "changeset_id": changeset, "open": true})),
        None => Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(
                "本会话已经有一个打开的变更集：先 commit_changeset 或 abort_changeset".to_owned(),
            ),
        )),
    }
}

/// **Blob 三级生命周期与孤儿回收** ✓（设计 §6.3 ✓）。
///
/// **安全默认** ✓：`dry_run` 缺省为 **true** ✓ ⇒ 只报告 ✓。
/// 真删要求**两个**条件同时成立 ✓（`dry_run: false` **且** `confirm: true` ✓）——
/// 删除是**不可逆**的 ✓，本项目"不做不可逆动作"的纪律在这里落成一对明确的开关 ✓。
/// 而且**只会删"已过 TTL 的孤儿"** ✓：活跃与历史一律不动 ✓（设计原话：GC 根集 = 全日志原子引用闭包 ✓）。
fn write_blob_gc(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let dry_run = optional_bool(args, "dry_run").unwrap_or(true);
    let confirm = optional_bool(args, "confirm").unwrap_or(false);
    // **降冷**也是显式动作 ✓（它虽然不删数据 ✓，但会改变数据的存放位置 ✓ ⇒ 同样默认不做 ✓）。
    let demote = optional_bool(args, "demote").unwrap_or(false);
    let ttl_days = optional_u64(args, "ttl_days").unwrap_or(7) as i64;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0);
    let report = ctx.workspace.blob_lifecycle(ttl_days, now_ms)?;
    // **拒绝"想删但没确认"** ✓：明确告诉调用方少了什么 ✓，而不是静默什么都不做 ✗。
    if !dry_run && !confirm {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "回收是不可逆的：需要 dry_run: false **与** confirm: true 同时给".to_owned(),
            ),
        ));
    }
    let mut response = json!({
        "ok": true,
        "dry_run": dry_run,
        "ttl_days": ttl_days,
        // **考虑了几份文档** ✓：这个数若小于磁盘上的文档数 ⇒ 分级不可信 ✓（勿信勿删 ✓）。
        "documents_considered": report.documents_considered,
        "active": {"blobs": report.active_count, "bytes": report.active_bytes},
        "history": {"blobs": report.history_count, "bytes": report.history_bytes},
        "orphan": {"blobs": report.orphan_count, "bytes": report.orphan_bytes},
        "collectible": {"blobs": report.collectible.len(), "bytes": report.collectible_bytes},
        // **清单** ✓：哪些 blob 在哪一级 ✓（审计用 ✓；测试靠它做"针对具体 blob"的断言 ✓）。
        "hashes": {
            "active": report.active_hashes,
            "history": report.history_hashes,
            "orphan": report.orphan_hashes,
            "collectible": report.collectible,
        },
    });
    // **冷层统计** ✓（设计要求可观测 ✓）。
    let (cold_count, cold_bytes) = ctx.workspace.store().cold_stats();
    response["cold"] = json!({"blobs": cold_count, "bytes": cold_bytes});
    if !dry_run {
        let (removed, freed) = ctx.workspace.collect_orphan_blobs(&report.collectible)?;
        response["removed"] = json!(removed);
        response["freed_bytes"] = json!(freed);
        if demote {
            // **降冷的对象是"历史级"** ✓ —— 按设计 ✓：被日志引用、但不在当前折叠状态 ✓
            //（revert 目标、被 declare_head 甩出、Stash 里、旧分支 ✓）⇒ 正是 `history_hashes` ✓。
            let (moved, moved_bytes) = ctx.workspace.store().demote(
                &report
                    .history_hashes
                    .iter()
                    .filter_map(|text| text.parse::<yanshi_core::BlobHash>().ok())
                    .collect::<Vec<_>>(),
            )?;
            response["demoted"] = json!(moved);
            response["demoted_bytes"] = json!(moved_bytes);
            let (cold_count, cold_bytes) = ctx.workspace.store().cold_stats();
            response["cold"] = json!({"blobs": cold_count, "bytes": cold_bytes});
        }
        response["note"] = json!(
            "只删了已过 TTL 的孤儿 ✓；活跃与历史一律保留 ✓（设计 6.3 ✓）；降冷只移动历史级 ✓，读路径会自动回退到归档 ✓"
        );
    }
    Ok(response)
}

/// **把工具参数解析成原子** ✓（`submit_offline` 用 ✓）。
///
/// **设计没规定离线原子在工具层怎么表达 ⇒ 记录选择** ✓：`{kind, payload, actor?, session?}` ✓ ——
/// `kind` 用与日志一致的序列化名 ✓（`"draw_stroke"` 之类 ✓），缺省 actor/session 取调用方 ✓。
fn parse_offline_atoms(ctx: &ToolContext<'_>, args: &Value) -> Result<Vec<Atom>> {
    let entries = require_array(args, "atoms")?;
    if entries.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("atoms 不能为空".to_owned()),
        ));
    }
    let mut atoms = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let kind_value = entry.get("kind").cloned().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("atoms[{index}] 缺少 kind")),
            )
        })?;
        let kind: AtomKind = serde_json::from_value(kind_value).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("atoms[{index}] 的 kind 无法识别：{error}")),
            )
        })?;
        let payload = entry.get("payload").cloned().unwrap_or_else(|| json!({}));
        let actor = entry
            .get("actor")
            .and_then(Value::as_str)
            .unwrap_or(&ctx.actor)
            .to_owned();
        let session = entry
            .get("session")
            .and_then(Value::as_str)
            .unwrap_or(&ctx.session)
            .to_owned();
        atoms.push(Atom::new(kind, actor, session, payload));
    }
    Ok(atoms)
}

/// 把校验错误写成一句人看得懂的话 ✓（"分支对比"里要显示它 ✓）。
fn stash_reason(error: &YanshiError) -> String {
    match error.context.detail.as_deref() {
        Some(detail) => format!("{:?}：{detail}", error.code),
        None => format!("{:?}", error.code),
    }
}

/// **重连时提交离线期间的原子** ✓（设计 §12.4 ✓）。
///
/// **整批要么全进日志、要么一条都不进** ✓（先 `validate_batch` 在状态副本上增量校验 ✓）——
/// 逐条提交的话 ✓，第三条失败时前两条**已经进日志** ✗，那就不是"打包"而是"半途而废" ✗。
///
/// **校验失败 ⇒ 搁置（不自动 Rebase）** ✓（§897 ✓），并**如实返回** `stashed: true` ✓ +
/// 原因 ✓ —— 调用方必须知道"这批原子没进日志 ✓、去哪找它们 ✓"，而不是只看到一句错误 ✗。
fn write_submit_offline(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let atoms = parse_offline_atoms(ctx, args)?;
    let count = atoms.len();
    let validation = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        document.validate_batch(&atoms, &ctx.actor, ctx.owner)
    };
    match validation {
        Ok(()) => {
            // 全部通过 ⇒ 一个变更集整批追加 ✓（"这一次重连"于是可整体撤销 ✓）。
            let changeset = yanshi_core::Changeset::new_id();
            let results = ctx.workspace.commit_changeset(
                &ctx.doc_id,
                atoms,
                &ctx.actor,
                ctx.owner,
                changeset.clone(),
            )?;
            let seqs: Vec<u64> = results.iter().map(|result| result.seq).collect();
            Ok(json!({
                "ok": true,
                "stashed": false,
                "applied": results.len(),
                "changeset_id": changeset,
                "seqs": seqs,
            }))
        }
        Err(error) => {
            let reason = stash_reason(&error);
            let stash_id =
                ctx.workspace
                    .stash(&ctx.doc_id, &ctx.actor, &ctx.session, &reason, atoms)?;
            Ok(json!({
                "ok": true,
                "stashed": true,
                "applied": 0,
                "atoms": count,
                "stash_id": stash_id,
                "reason": reason,
            }))
        }
    }
}

/// **列出悬空变更集** ✓（§898 的"分支对比"要的素材 ✓）。
fn read_list_stashes(ctx: &mut ToolContext<'_>, _args: &Value) -> Result<Value> {
    let entries: Vec<Value> = ctx
        .workspace
        .stashes()
        .into_iter()
        .map(|stash| {
            json!({
                "stash_id": stash.id,
                "doc_id": stash.doc_id,
                "actor": stash.actor,
                "created_at": stash.created_at,
                "reason": stash.reason,
                "atoms": stash.atoms.len(),
                "blob_refs": stash.blob_refs.len(),
            })
        })
        .collect();
    Ok(json!({"ok": true, "stashes": entries, "count": entries.len()}))
}

/// **强制应用到当前 HEAD** ✓（§898 的第二种上层选择 ✓ —— 设计自己注明"**可能产生视觉错误**" ✓，
/// 所以它**必须**是显式动作 ✓、绝不能自动发生 ✓）。
///
/// 失败时**原样留着** ✓（不取走 ✓）：重放失败说明它现在仍然应用不上 ✓ ⇒
/// 丢掉它会**毁掉用户离线期间的工作** ✗（那是本项目最不能接受的失败方式 ✓）。
fn write_apply_stash(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let stash_id = require_str(args, "stash_id")?;
    let entry = ctx
        .workspace
        .stashes()
        .into_iter()
        .find(|stash| stash.id == stash_id)
        .cloned()
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("悬空变更集 {stash_id} 不存在")),
            )
        })?;
    {
        let document = ctx.workspace.document(&entry.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", entry.doc_id)),
            )
        })?;
        if let Err(error) = document.validate_batch(&entry.atoms, &ctx.actor, ctx.owner) {
            return Ok(json!({
                "ok": false,
                "applied": 0,
                "stash_id": stash_id,
                "reason": stash_reason(&error),
                "kept": true,
                "note": "重放仍然过不了校验 ⇒ 悬空变更集原样保留（不毁掉离线期间的工作）",
            }));
        }
    }
    let changeset = yanshi_core::Changeset::new_id();
    let results = ctx.workspace.commit_changeset(
        &entry.doc_id,
        entry.atoms.clone(),
        &ctx.actor,
        ctx.owner,
        changeset.clone(),
    )?;
    ctx.workspace.take_stash(&stash_id)?;
    Ok(json!({
        "ok": true,
        "applied": results.len(),
        "stash_id": stash_id,
        "changeset_id": changeset,
        "note": "按设计 12.4：强制应用可能产生视觉错误",
    }))
}

/// **丢弃悬空变更集** ✓（§898 的第一种上层选择 ✓）。
///
/// **只丢"待重放"这件事** ✓，**不删 blob** ✗：§899 说 Stash 里的 blob 归历史级保留、不被 GC ✓ ——
/// 抹掉历史不是"丢弃"的意思 ✓（本项目也没有 GC ✓）。
fn write_discard_stash(ctx: &mut ToolContext<'_>, _args: &Value) -> Result<Value> {
    let stash_id = require_str(_args, "stash_id")?;
    let taken = ctx.workspace.take_stash(&stash_id)?;
    match taken {
        Some(entry) => Ok(json!({
            "ok": true,
            "discarded": true,
            "stash_id": stash_id,
            "atoms": entry.atoms.len(),
            "blobs_kept": entry.blob_refs.len(),
        })),
        None => Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("悬空变更集 {stash_id} 不存在")),
        )),
    }
}

/// **只读导入 PSD 的合成图** ✓（设计第 17 章把它列为后续项 ✓ —— 这一轮实现 ✓）。
///
/// **契约** ✓（与模块文档一致 ✓）：只取文件里那张**已经合成好的整幅图** ✓，
/// **不导入图层结构** ✗（PSD 的图层、蒙版、混合模式、智能对象都不搬 ✓）、**不写回 PSD** ✗。
/// 结果作为**一个** `raster_patch` 交出去 ✓ ⇒ 与"导入一张 PNG"走**同一条**下游路径 ✓
///（可撤销 ✓、可回放 ✓、介质描述符照记 ✓）。
///
/// **三步都不可省** ✓：把 PSD 字节解成 RGBA ✓ ⇒ **先把 RGBA 落成新 blob** ✓（blob 先行 ✓，
/// 否则原子会引用一个还不存在的 hash ✗）⇒ **复用 `import_image`** ✓（不复制它的逻辑 ✓）。
///
/// **不支持的一律明确报错** ✗（版本 2 ✓、16/32 位 ✓、CMYK/灰度 ✓、ZIP 压缩 ✓、截断 ✓）
/// —— **绝不"尽力而为地画一半"** ✗：半个画面比一句明确的错误更糟 ✓。
fn write_import_psd(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let blob_hash = require_str(args, "blob_hash")?;
    let parsed: yanshi_core::BlobHash = blob_hash.parse().map_err(|_| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("blob_hash 不是合法哈希"),
        )
    })?;
    let bytes = ctx.workspace.store().get(&parsed)?;
    let (width, height, rgba) = yanshi_render::psd::decode_psd(&bytes).map_err(|error| {
        // **原样回报解析器的原因** ✓ —— 它已经写得足够具体 ✓（"位深 16 不支持" 之类 ✓）。
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("PSD 解析失败：{}", error.message)),
        )
    })?;
    // **blob 先行** ✓：先把合成图写进存储 ✓，再提交引用它的原子 ✓。
    let composite = ctx.workspace.store().put(&rgba)?;
    let mut import_args = json!({
        "layer_id": args.get("layer_id").cloned().unwrap_or(Value::Null),
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": 0.0, "y": 0.0, "w": width as f64, "h": height as f64},
    });
    if let Some(object_id) = args.get("object_id") {
        import_args["object_id"] = object_id.clone();
    }
    let mut value = write_import_image(ctx, &import_args)?;
    value["source"] = json!({"format": "psd", "width": width, "height": height,
                             "note": "只导入了合成图；图层结构与蒙版没有导入（只读）"});
    Ok(value)
}

/// **重采样一个光栅对象** ✓（设计 777 的 `resample` ✓）。
///
/// **设计只给了名字 ⇒ 记录选择** ✓（详见 `yanshi_core::resample` 的模块说明 ✓）：
/// 对象 = `raster_patch` ✓；输入只接受**原始 RGBA** ✓（其它 mime 明确拒绝 ✓）；
/// 产出**新 blob + `supersede`** ✓ ⇒ **非破坏** ✓（旧原子留在日志 ✓、可撤销 ✓、可回放 ✓）。
/// **缺省 `bilinear`** ✓ —— 介质笔触是连续调 ✓；像素画调用方请显式选 `nearest` ✓。
///
/// **两条不变量** ✓：
/// * **blob 先行** ✓（先把新位图写进存储 ✓，再提交引用它的原子 ✓ —— 否则校验会拒 ✓）；
/// * **介质描述符原样保留** ✓，并补上 `resampled_from` 记录来源与滤镜 ✓ ——
///   设计 11.1 要求"插件 id + version 随原子记录 ✓、升级不自动改变旧文档渲染" ✓，
///   而重采样**改了像素** ✓ ⇒ 必须让后来人能看出"这不是插件当初画的那份" ✓。
fn write_resample(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let filter_name = optional_str(args, "filter").unwrap_or_else(|| "bilinear".to_owned());
    let filter = yanshi_core::resample::ResampleFilter::parse(&filter_name).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("未知滤镜 {filter_name}（可用 nearest / bilinear）")),
        )
    })?;
    // ① 读对象 ✓：类型、位图、尺寸 ✓。
    let (data, source_width, source_height, source_hash) = {
        let state = document_state(ctx)?;
        let object = state.objects.get(object_id.as_str()).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            )
        })?;
        if object.is_deleted() {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 已删除")),
            ));
        }
        if object.object_type != ObjectType::RasterPatch {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "resample 只作用于光栅对象（raster_patch）；{object_id} 的类型是 {:?}",
                    object.object_type
                )),
            ));
        }
        let bitmap = object.data.get("bitmap").cloned().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("对象 {object_id} 没有 bitmap")),
            )
        })?;
        let mime = bitmap
            .get("mime_type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if mime != "image/x-yanshi-raw" {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "resample 只接受原始 RGBA 位图（image/x-yanshi-raw）；{object_id} 的是 {mime} \
                     —— 转码是另一件事，本工具不猜"
                )),
            ));
        }
        let hash = bitmap
            .get("blob_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("对象 {object_id} 的 bitmap 缺少 blob_hash")),
                )
            })?
            .to_owned();
        let width = object
            .data
            .get("width")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let height = object
            .data
            .get("height")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        if width == 0 || height == 0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("对象 {object_id} 的 width/height 缺失或为 0")),
            ));
        }
        (object.data.clone(), width, height, hash)
    };
    // ② 目标尺寸 ✓：显式宽高优先 ✓，其次 scale ✓ —— 两个都不给就明确报错 ✗（不猜 ✗）。
    let scale = args.get("scale").and_then(Value::as_f64);
    let explicit_w = args.get("width").and_then(Value::as_u64).map(|v| v as u32);
    let explicit_h = args.get("height").and_then(Value::as_u64).map(|v| v as u32);
    let (target_w, target_h) = match (explicit_w, explicit_h, scale) {
        (Some(w), Some(h), _) => (w, h),
        (None, None, Some(factor)) if factor > 0.0 => (
            ((source_width as f64 * factor).round() as u32).max(1),
            ((source_height as f64 * factor).round() as u32).max(1),
        ),
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "resample 需要 width 与 height，或 scale（二者之一，明确给 ✓）".to_owned(),
                ),
            ))
        }
    };
    if target_w == 0 || target_h == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("目标尺寸不能为 0".to_owned()),
        ));
    }
    if target_w == source_width
        && target_h == source_height
        && filter == yanshi_core::resample::ResampleFilter::Nearest
    {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("同尺寸 + nearest 是恒等变换 ⇒ 不提交无意义的原子".to_owned()),
        ));
    }
    // ③ 取像素、重采样、**先把新 blob 写进存储** ✓（blob 先行 ✓）。
    let source_hash: yanshi_core::BlobHash = source_hash.parse().map_err(|_| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("对象位图的 blob_hash 不是合法哈希".to_owned()),
        )
    })?;
    let source = ctx.workspace.store().get(&source_hash)?;
    let resampled = yanshi_core::resample::resample_rgba(
        &source,
        source_width,
        source_height,
        target_w,
        target_h,
        filter,
    )
    .ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "位图与尺寸不符：像素 {} 字节，声明 {source_width}×{source_height}（应为 {} 字节）",
                source.len(),
                source_width as usize * source_height as usize * 4
            )),
        )
    })?;
    let new_hash = ctx.workspace.store().put(&resampled)?;
    // ④ 提交 `supersede` ✓：保留介质描述符 ✓，并记下"这份像素是重采样来的" ✓。
    let mut new_data = data.clone();
    let region = data.get("region").cloned().unwrap_or_else(
        || json!({"x": 0.0, "y": 0.0, "w": source_width as f64, "h": source_height as f64}),
    );
    new_data["bitmap"] = json!({
        "blob_hash": new_hash.to_string(),
        "size": resampled.len(),
        "mime_type": "image/x-yanshi-raw",
    });
    new_data["width"] = json!(target_w);
    new_data["height"] = json!(target_h);
    new_data["region"] = json!({
        "x": region.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        "y": region.get("y").and_then(Value::as_f64).unwrap_or(0.0),
        "w": target_w as f64,
        "h": target_h as f64,
    });
    new_data["resampled_from"] = json!({
        "blob_hash": source_hash.to_string(),
        "width": source_width,
        "height": source_height,
        "filter": filter.name(),
    });
    let result = ctx.commit(
        AtomKind::Supersede,
        json!({"object_id": object_id, "data": new_data, "type": "raster_patch"}),
    )?;
    let bbox = Bbox::new(0.0, 0.0, target_w as f64, target_h as f64);
    let mut value = finish_mutation(ctx, &result, Some(bbox))?;
    value["object_id"] = json!(object_id);
    value["blob_hash"] = json!(new_hash.to_string());
    value["source"] = json!({"width": source_width, "height": source_height, "blob_hash": source_hash.to_string()});
    value["target"] = json!({"width": target_w, "height": target_h});
    value["filter"] = json!(filter.name());
    Ok(value)
}

/// **解决采样性替换冲突** ✓（设计 12.3 ✓ —— 设计把它写得非常明确 ✓，这一节是全篇最清楚的之一 ✓）。
///
/// **设计原话** ✓：`resolve_conflict` 是**组合宏、不是新原子类型** ✓ —— **折叠器零改动** ✓。
/// 因此这里**只用既有原子种类** ✓ 展开出四种手段 ✓：
///
/// | resolution | 展开为 |
/// |---|---|
/// | `keep_ours` | 撤销对方原子 ✓ + 把我们的对象移回正式图层 ✓ |
/// | `keep_theirs` | 撤销我们的对象 ✓ |
/// | `discard` | 撤销双方 ✓ |
/// | `merge` | **内容不动** ✓，只关闭冲突（上层自己编辑后再提交 ✓） |
///
/// **两处"照着设计做、但要把话说清"的地方** ✓：
/// * 设计写的是 `tombstone(对方原子)` ✓ —— 但**原子不可墓碑化**（日志追加式 ✓，`tombstone` 的对象是
///   对象/图层/选区 ✓）⇒ 这里用**既有机制 `Revert {target: 对方原子}`** 实现同一个意思
///   "让对方的原子失效" ✓（这正是设计"组合宏、折叠器零改动"的本意 ✓）；
/// * 设计写的是 `move(我方原子 → 正式图层)` ✓ —— 同样地，原子的归属不可改 ✓，
///   改的是它**建出来的对象** ✓ ⇒ 用 `set_property {key: "layer_id"}` ✓（既有工具 ✓）。
///
/// **冲突图层在解决后 tombstone** ✓，而 `metadata.conflict` 标记**保留供审计** ✓（设计原话 ✓）——
/// 因为 `Tombstone` 只标记"不再使用" ✓，不改元数据 ✓。
///
/// **整个宏归一个变更集** ✓ ⇒ 一次解决动作可整体撤销 ✓。
fn write_resolve_conflict(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let resolution = require_str(args, "resolution")?;
    const RESOLUTIONS: [&str; 4] = ["keep_ours", "keep_theirs", "discard", "merge"];
    if !RESOLUTIONS.contains(&resolution.as_str()) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "未知的 resolution {resolution}（可用：{}）",
                RESOLUTIONS.join(" / ")
            )),
        ));
    }
    // **找到冲突图层** ✓：优先用调用方给的 ✓；否则找**唯一**的那个（多个就报错 ✓，不猜 ✗）。
    let conflict_layer_id = match optional_str(args, "conflict_layer_id") {
        Some(id) => id,
        None => {
            let state = document_state(ctx)?;
            let found: Vec<String> = state
                .alive_layers()
                .iter()
                .filter(|layer| {
                    layer
                        .metadata
                        .get("conflict")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .map(|layer| layer.id.clone())
                .collect();
            match found.len() {
                1 => found[0].clone(),
                0 => {
                    return Err(YanshiError::new(
                        ErrorCode::ReferenceNotFound,
                        ErrorContext::detail(
                            "找不到冲突图层（metadata.conflict = true 的图层）".to_owned(),
                        ),
                    ))
                }
                _ => {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "有 {} 个冲突图层，请用 conflict_layer_id 指明（不猜 ✗）",
                            found.len()
                        )),
                    ))
                }
            }
        }
    };
    // **我方提交 = 冲突图层上的原子** ✓（§876：客户端把重提交的原子改投冲突图层 ✓）。
    //
    // **这里有一处设计与本仓库模型的落差，如实记下** ✓：设计表写的是 `tombstone(我方原子)` 与
    // `move(我方原子 → 正式图层)` ✓ —— 但在本仓库里 **原子既不能墓碑化、归属也不可改** ✓
    //（日志追加式 ✓）。而且 `liquify`/`retouch` 的提交是**图层上的像素原子**（没有对象 ✗），
    // 设计表里那两行的字面实现**不存在** ✗。所以按**本仓库真正有的机制**做同一件事 ✓，
    // 并且对**两种形态都成立** ✓：
    // * 让一个原子失效 ⇒ `Revert {target}` ✓；
    // * 把提交"搬"到正式图层 ⇒ 若它建了**对象** 就改对象的 `layer_id` ✓；
    //   若它是**原子型**（像素补丁 ✓）就把**同样的种类与载荷**改投正式图层重提交 ✓ ——
    //   这才是"move"在这个模型里的对应物 ✓。
    let (ours, our_objects) = {
        let state = document_state(ctx)?;
        if !state.layers.contains_key(conflict_layer_id.as_str()) {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("冲突图层 {conflict_layer_id} 不存在")),
            ));
        }
        let objects: Vec<String> = state
            .objects
            .values()
            .filter(|object| !object.is_deleted() && object.layer_id == conflict_layer_id)
            .map(|object| object.id.clone())
            .collect();
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        // **排除系统原子** ✓：冲突图层本身是 `system:conflict` 建的 ✓ ⇒ 它不是"我方提交" ✗。
        let atoms: Vec<(String, AtomKind, Value)> = document
            .log()
            .iter()
            .filter(|atom| atom.actor != yanshi_core::conflict::CONFLICT_ACTOR)
            .filter(|atom| {
                atom.payload.get("layer_id").and_then(Value::as_str)
                    == Some(conflict_layer_id.as_str())
            })
            .map(|atom| (atom.id.clone(), atom.kind, atom.payload.clone()))
            .collect();
        (atoms, objects)
    };
    let opponent = optional_str(args, "opponent_atom_id");
    let formal = optional_str(args, "formal_layer_id");
    if matches!(resolution.as_str(), "keep_ours" | "discard") && opponent.is_none() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "{resolution} 需要 opponent_atom_id（冲突错误里带的是哪个原子 ✓）"
            )),
        ));
    }
    if resolution == "keep_ours" && formal.is_none() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "keep_ours 需要 formal_layer_id：我们的对象要移回哪个正式图层".to_owned(),
            ),
        ));
    }
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let mut moved = 0usize;
    let mut tombstoned = 0usize;
    let mut reverted_opponent = false;
    let mut failure: Option<YanshiError> = None;
    // ① 我们这一侧 ✓。
    if resolution == "keep_theirs" || resolution == "discard" {
        // 让**我方的每一步提交**失效 ✓（对象型的先墓碑化它的对象 ✓，再对所有我方原子发 `Revert` ✓）。
        for object_id in &our_objects {
            if let Err(error) = ctx.commit(AtomKind::Tombstone, json!({"object_id": object_id})) {
                failure = Some(error);
                break;
            }
        }
        if failure.is_none() {
            for (atom_id, _, _) in &ours {
                match ctx.commit(AtomKind::Revert, json!({"target": atom_id})) {
                    Ok(_) => tombstoned += 1,
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
        }
    } else if resolution == "keep_ours" {
        let formal = formal.clone().unwrap_or_default();
        // 对象型提交 ⇒ 移对象 ✓（设计表的 `move` ✓）。
        for object_id in &our_objects {
            match ctx.commit(
                AtomKind::SetProperty,
                json!({"object_id": object_id, "key": "layer_id", "value": formal}),
            ) {
                Ok(_) => moved += 1,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        // 原子型提交（像素补丁 ✓）⇒ **改投正式图层重提交** ✓ —— 见上面那段说明 ✓。
        //
        // **建对象的那些原子必须排除** ✗：它们的"搬移"已经由上面改对象 `layer_id` 完成了 ✓
        //（我第一版没排除 ⇒ 又重提交了一次 `create_object` ✓ ⇒ 报"对象已存在" ✗，
        //  测试当场抓到 ✓）。判据取**载荷里的 object_id 是否就是我方对象之一** ✓ ——
        // 这仍是从载荷形状判断 ✓，不是枚举原子种类 ✗（后者迟早漏 ✓）。
        if failure.is_none() {
            let object_ids: std::collections::BTreeSet<&String> = our_objects.iter().collect();
            for (atom_id, kind, payload) in &ours {
                let created_object = payload
                    .get("object_id")
                    .and_then(Value::as_str)
                    .map(|id| object_ids.contains(&id.to_owned()))
                    .unwrap_or(false);
                if created_object {
                    continue;
                }
                let mut moved_payload = payload.clone();
                moved_payload["layer_id"] = json!(formal);
                match ctx.commit(*kind, moved_payload) {
                    Ok(_) => {
                        moved += 1;
                        // 原地的那一份要失效 ✓，否则同一笔会在两个图层上各生效一次 ✗。
                        if let Err(error) = ctx.commit(AtomKind::Revert, json!({"target": atom_id}))
                        {
                            failure = Some(error);
                            break;
                        }
                    }
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
        }
    }
    // ② 对方那一侧 ✓（`keep_ours`/`discard` ⇒ 让对方的原子失效 ✓；`tombstone(对方原子)` 的实现见上面的说明 ✓）。
    if failure.is_none() && matches!(resolution.as_str(), "keep_ours" | "discard") {
        if let Some(target) = opponent.clone() {
            match ctx.commit(AtomKind::Revert, json!({"target": target})) {
                Ok(_) => reverted_opponent = true,
                Err(error) => failure = Some(error),
            }
        }
    }
    // ③ **关闭冲突：墓碑化冲突图层** ✓（元数据保留供审计 ✓，设计原话 ✓）。
    if failure.is_none() {
        match ctx.commit(AtomKind::Tombstone, json!({"layer_id": conflict_layer_id})) {
            Ok(_) => {}
            Err(error) => failure = Some(error),
        }
    }
    ctx.changeset = previous;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(json!({
        "ok": true,
        "resolution": resolution,
        "conflict_layer_id": conflict_layer_id,
        "our_submissions": ours.len(),
        "our_objects": our_objects.len(),
        "moved_to_formal": moved,
        "tombstoned_ours": tombstoned,
        "reverted_opponent": reverted_opponent,
        "changeset_id": changeset,
    }))
}

/// **开始一个事务** ✓（设计 777 ✓；设计只给了名字 ⇒ 语义见 `Workspace::transactions` 的说明 ✓）。
fn write_begin_transaction(ctx: &mut ToolContext<'_>, _args: &Value) -> Result<Value> {
    match ctx.workspace.begin_transaction(&ctx.doc_id, &ctx.session) {
        Some(changeset) => Ok(json!({"ok": true, "changeset_id": changeset, "transaction": true})),
        None => Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(
                "本会话已经有一个打开的变更集或事务：先 commit/abort 收尾".to_owned(),
            ),
        )),
    }
}

/// **收尾当前事务** ✓：原子保留 ✓，且它们同属一个变更集 ⇒ 可作为一组撤销 ✓。
fn write_commit_transaction(ctx: &mut ToolContext<'_>, _args: &Value) -> Result<Value> {
    let Some(changeset) = ctx.workspace.transaction(&ctx.doc_id, &ctx.session) else {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("本会话没有打开的事务（先 begin_transaction）".to_owned()),
        ));
    };
    let atoms = changeset_atom_count(ctx, &changeset)?;
    ctx.workspace.close_transaction(&ctx.doc_id, &ctx.session);
    Ok(json!({"ok": true, "changeset_id": changeset, "atoms": atoms, "transaction": false}))
}

/// **收尾当前变更集** ✓：原子**保留** ✓，只是此后不再并入 ✓。
fn write_commit_changeset(ctx: &mut ToolContext<'_>, _args: &Value) -> Result<Value> {
    let Some(changeset) = ctx.workspace.close_changeset(&ctx.doc_id, &ctx.session) else {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("本会话没有打开的变更集（先 begin_changeset）".to_owned()),
        ));
    };
    let atoms = changeset_atom_count(ctx, &changeset)?;
    Ok(json!({"ok": true, "changeset_id": changeset, "atoms": atoms, "open": false}))
}

/// **放弃当前变更集** ✓：把它里面的原子**整体撤销** ✓，然后关闭 ✓。
///
/// **设计未规定"放弃"是删除还是撤销 ⇒ 记录选择** ✓：**撤销** ✓ ——
/// 本项目的日志是**追加式**的 ✓（删除不是一种操作 ✓），撤销既保留历史 ✓、
/// 又能让"放弃"这件事本身**可再撤销** ✓（撤销归入一个新的变更集 ✓）。
fn write_abort_changeset(ctx: &mut ToolContext<'_>, _args: &Value) -> Result<Value> {
    let Some(changeset) = ctx.workspace.close_changeset(&ctx.doc_id, &ctx.session) else {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("本会话没有打开的变更集（先 begin_changeset）".to_owned()),
        ));
    };
    // 先关闭再撤销 ✓：撤销自己也要归入**新的**变更集 ✓，绝不能并进正在被撤销的那个 ✗。
    let (reverted, revert_changeset_id) = revert_changeset_atoms(ctx, &changeset)?;
    Ok(
        json!({"ok": true, "changeset_id": changeset, "reverted": reverted,
              "revert_changeset_id": revert_changeset_id, "open": false}),
    )
}

/// 数一个变更集里有多少条原子 ✓（只读 ✓）。
fn changeset_atom_count(ctx: &ToolContext<'_>, changeset_id: &str) -> Result<usize> {
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    Ok(document
        .log()
        .iter()
        .filter(|atom| atom.changeset_id.as_deref() == Some(changeset_id))
        .count())
}

/// **撤销一个变更集里的全部原子** ✓ —— `revert_changeset` 与 `abort_changeset` 共用这一份 ✓。
///
/// **为什么抽出来** ✓：这两个工具的语义只差"从哪拿到 changeset_id" ✓（一个来自调用方参数 ✓、
/// 一个来自打开的会话 ✓）⇒ 撤销逻辑**只应有一份** ✓（"同一事实两处各写一遍"迟早分叉 ✗）。
fn revert_changeset_atoms(
    ctx: &mut ToolContext<'_>,
    changeset_id: &str,
) -> Result<(usize, yanshi_core::ChangesetId)> {
    let targets: Vec<String> = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        document
            .log()
            .iter()
            .filter(|atom| atom.changeset_id.as_deref() == Some(changeset_id))
            .map(|atom| atom.id.clone())
            .collect()
    };
    // **所有 revert 归一个变更集** ✓ ⇒ 这一次撤销本身也可被一次撤销 ✓（调用方还会拿到它的 id ✓）。
    let changeset = yanshi_core::Changeset::new_id();
    let previous = ctx.changeset.replace(changeset.clone());
    let mut reverted = 0usize;
    let mut failure: Option<YanshiError> = None;
    for target in &targets {
        match ctx.commit(AtomKind::Revert, json!({"target": target})) {
            Ok(_) => reverted += 1,
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    ctx.changeset = previous;
    match failure {
        Some(error) => Err(error),
        None => Ok((reverted, changeset)),
    }
}

/// **列出变更集** ✓（设计 793 的 `revert_changeset` 要撤销的对象）。
///
/// 为什么这一对很重要 ✓：本项目从第 36 轮起**已经在用变更集** ✓
///（`detach_instance` ✓ 与 `path_edit join` ✓ 的两步原子 ✓ 都属于同一个变更集 ✓），
/// 当时的说明写着"半成品能**一次整体撤销**" ✗ —— 而**当时并没有撤销它的工具** ✗✓。
/// 也就是说那句承诺**兑现不了** ✓：这正是一处**我自己制造的诚实缺口** ✓，
/// 本轮把它补上 ✓（`get_changesets` 看得到 ✓、`revert_changeset` 撤得掉 ✓）。
fn write_path_edit(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let op = require_str(args, "op")?;
    let object_id = require_str(args, "object_id")?;
    // **这里原本有一个 `unsupported` 闭包** ✓ —— 它专门回答"该算子需要设计尚未规定的路径对象模型" ✓。
    // **里程碑** ✓：设计 792 的**八个算子现在全部实现了** ✓
    //（`reverse`/`close`/`join`/`merge`/`split`/`convert_to_shape`/`convert_to_path`/`boolean` ✓）
    // ⇒ 这个闭包**再也不会被调用** ✓ ⇒ 删掉 ✓（clippy 的"未使用"提示正好点出这一点 ✓）。
    match op.as_str() {
        "reverse" | "close" => {}
        "join" | "merge" => {}
        // **`split` 在下一段单独处理** ✓（它要读 `at` ✓ 且**不修改**原对象 ✓
        // —— 它是唯一一个"一个对象变两个"的算子 ✓，走另一条路更清楚 ✓）。
        "split" => {}
        // **`convert_to_shape` 委托给同一个实现** ✓ —— 设计把它列在 `path_edit` 的算子表里 ✓，
        // 同时它也是独立工具 ✓ ⇒ 两个入口**必须不分叉** ✓（各自实现一遍是本项目反复吃亏的地方 ✓）。
        "convert_to_shape" => {
            return write_convert_to_shape(ctx, args);
        }
        // **`boolean`** ✓（设计 792 的最后一个算子 ✓）：几何在 `yanshi_render::polygon` ✓，
        // 这里只负责"取几何 ⇒ 算 ⇒ 落成对象" ✓。
        "boolean" => {
            return write_path_boolean(ctx, args);
        }
        // **`convert_to_path` 委托给独立工具** ✓ —— 设计把它列在 `path_edit` 的算子表里 ✓，
        // 同时它也是独立工具 ✓ ⇒ 两个入口**不应分叉** ✓（与 `convert_to_shape` 同一处理 ✓）。
        //
        // **此前它是拒绝的** ✗，理由写的是"路径 ⇒ 路径没有意义" ✓ —— 那句话本身没错 ✓，
        // 但**笔迹 ⇒ 路径**恰恰是这个算子的用处 ✓（用户会把一条笔迹当"路径"来转 ✓），
        // 所以拒绝是**过度限制**了 ✓。现在：笔迹 ⇒ 转换 ✓；路径 ⇒ 工具自己给出明确报错 ✓
        //（"路径转路径无需转换" ✓），两处语义一致 ✓。
        "convert_to_path" => {
            return write_convert_to_path(ctx, args);
        }
        other => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "未知的 path_edit 算子 {other}\
                     （已实现：reverse / close / join / merge / split / convert_to_shape / boolean）"
                )),
            ));
        }
    }

    // **`split` 单独走一条路** ✓（设计 792 只给了名字 ⇒ 记录选择 ✓）：
    // 在**节点下标** `at` 处把一条**开放**路径切成两条 ✓，两半各自复制切口节点 ✓。
    // * **切口节点的控制柄按"归属"分** ✓：左半的末节点保留 `in`、`out` 归零 ✓；
    //   右半的首节点保留 `out`、`in` 归零 ✓ —— 这样**两半合起来与原曲线完全一致** ✓
    //   （测试用"两半的墨量之和 = 原来的墨量"守住 ✓）；
    // * **闭合路径拒绝** ✓：闭合环在**一个**节点处是切不开的 ✓（数学上需要两刀 ✓），
    //   而设计没说第二刀怎么给 ✓ ⇒ 明确报错 ✓，不擅自发明 ✗。
    if op == "split" {
        return write_path_split(ctx, args);
    }

    // 读目标笔迹的点 ✓（对象类型必须是笔迹 ✓ —— 形状/文本没有"点序"可言 ✓）。
    let (object_type, mut points, data) = {
        let state = document_state(ctx)?;
        let Some(object) = state.objects.get(object_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {object_id} 不存在")),
            ));
        };
        // **路径与笔迹都支持** ✓（设计 792 的 `path_edit` 本就该作用于路径 ✓；
        // 笔迹是它的前身 ✓，两者的样式字段相同 ✓ ⇒ 两条分支共用算子语义 ✓）。
        match object.object_type {
            yanshi_core::ObjectType::Stroke => {
                let points: Vec<Value> = object
                    .data
                    .get("points")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                (object.object_type, points, object.data.clone())
            }
            yanshi_core::ObjectType::Path => {
                let nodes: Vec<Value> = object
                    .data
                    .get("nodes")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                (object.object_type, nodes, object.data.clone())
            }
            _ => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{object_id} 既不是笔迹也不是路径（path_edit 作用于它们的点/节点）"
                    )),
                ))
            }
        }
    };
    // **路径还是笔迹** ✓ —— 两条分支的算子语义不同 ✓，此处提前定义，后面的算子与报错都要用 ✓。
    let is_path = object_type == yanshi_core::ObjectType::Path;
    if points.is_empty() {
        // 报错要说**这个对象该有什么** ✓（路径没有 nodes 与笔迹没有 points 是两件事 ✓）。
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(if is_path {
                format!("{object_id} 没有 nodes")
            } else {
                format!("{object_id} 没有 points")
            }),
        ));
    }

    // **路径与笔迹的算子语义不同** ✓ —— 这一点很容易做错 ✓：
    // * 笔迹只有点 ✓：reverse 就是反转点序 ✓、close 是**把首点接到末尾** ✓；
    // * 路径有节点 + 控制柄 ✓：reverse **必须同时交换每个节点的 in/out** ✓
    //   —— 只反转节点会让曲线**悄悄变形** ✗（形状还在、但弯的方向变了 ✓，最难发现的那类 bug ✓）；
    //   close 是**置 `closed` 标志** ✓（不是追加节点 ✓，否则会多出一段零长度曲线 ✗）。
    match op.as_str() {
        "reverse" => {
            if is_path {
                points.reverse();
                for node in points.iter_mut() {
                    // **交换 in/out** ✓：反向走同一条贝塞尔曲线 ⇒ 每个节点的进出控制柄互换 ✓。
                    if let Some(map) = node.as_object_mut() {
                        let (incoming, outgoing) = (
                            map.get("in").cloned().unwrap_or(json!([0.0, 0.0])),
                            map.get("out").cloned().unwrap_or(json!([0.0, 0.0])),
                        );
                        map.insert("in".to_owned(), outgoing);
                        map.insert("out".to_owned(), incoming);
                    }
                }
            } else {
                points.reverse();
            }
            let mut data = data;
            let key = if is_path { "nodes" } else { "points" };
            data[key] = json!(points);
            let result = ctx.commit(
                AtomKind::Supersede,
                json!({"object_id": object_id, "data": data}),
            )?;
            let head = result.head_seq;
            return Ok(json!({"ok": true, "op": "reverse", "object_id": object_id,
                             "kind": if is_path { "path" } else { "stroke" },
                             "count": points.len(), "head": head}));
        }
        "close" => {
            if is_path {
                let already = data.get("closed").and_then(Value::as_bool).unwrap_or(false);
                if already {
                    return Ok(json!({"ok": true, "op": "close", "object_id": object_id,
                                     "kind": "path", "closed": true, "changed": false}));
                }
                let mut data = data;
                data["closed"] = json!(true);
                let result = ctx.commit(
                    AtomKind::Supersede,
                    json!({"object_id": object_id, "data": data}),
                )?;
                let head = result.head_seq;
                return Ok(json!({"ok": true, "op": "close", "object_id": object_id,
                                 "kind": "path", "closed": true, "changed": true, "head": head}));
            }
            // 笔迹：**幂等** ✓（末点等于首点 ⇒ 什么都不做 ✓，但仍如实报告 `closed: true` ✓）。
            let already = points.len() >= 2 && points.first() == points.last();
            if already {
                return Ok(json!({"ok": true, "op": "close", "object_id": object_id,
                                 "kind": "stroke", "closed": true, "changed": false}));
            }
            let first = points[0].clone();
            points.push(first);
            let mut data = data;
            data["points"] = json!(points);
            let result = ctx.commit(
                AtomKind::Supersede,
                json!({"object_id": object_id, "data": data}),
            )?;
            let head = result.head_seq;
            return Ok(json!({"ok": true, "op": "close", "object_id": object_id,
                             "kind": "stroke", "closed": true, "changed": true,
                             "count": points.len(), "head": head}));
        }
        _ => {}
    }

    // `join` / `merge` ✓：接上第二条对象 ✓，然后把第二条 tombstone ✓（一个变更集 ✓）。
    //
    // **两者的差别（设计只给了两个名字 ⇒ 记录选择 ✓）**：
    // `join` 只是**接起来** ✓（拼接节点 ✓，两端的控制柄**原样保留** ✓）；
    // `merge` 额外把**接缝处的切线对齐** ✓ —— 以接缝弦长为尺度 ✓，给左半末节点设 `out` ✓、
    // 给右半首节点设 `in` ✓（各取弦长的 1/3 ✓，即三次贝塞尔表达直线的常用长度 ✓），
    // 使接缝处**平滑过渡** ✓ 而不是留下一个折角 ✓。
    // **为什么不做成同义词** ✗：设计把两个名字并列 ✓，若语义相同就应当合并成一个 ✓；
    // 这里给出"平滑接缝"这一条**可验证**的差别 ✓（测试比较两者的渲染确实不同 ✓）。
    let is_merge = op == "merge";
    let other_id = require_str(args, "other_id")?;
    if other_id == object_id {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("join 的两个对象不能是同一个".to_owned()),
        ));
    }
    let other_points: Vec<Value> = {
        let state = document_state(ctx)?;
        let Some(other) = state.objects.get(other_id.as_str()) else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("对象 {other_id} 不存在")),
            ));
        };
        // **join 的第二个对象必须与第一个同类** ✓ —— 路径接笔迹（或反之）没有明确语义 ✓，
        // 报错比"悄悄接上、样式来自第一个"诚实得多 ✓。
        if other.object_type != object_type {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{other_id} 与 {object_id} 不是同一类对象（join 要求同类：笔迹接笔迹、路径接路径）"
                )),
            ));
        }
        if !matches!(
            other.object_type,
            yanshi_core::ObjectType::Stroke | yanshi_core::ObjectType::Path
        ) {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{other_id} 既不是笔迹也不是路径")),
            ));
        }
        let key = if other.object_type == yanshi_core::ObjectType::Path {
            "nodes"
        } else {
            "points"
        };
        other
            .data
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    if other_points.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{other_id} 没有 points")),
        ));
    }
    // **接缝处重合的节点要去重** ✓ —— 两条路径"对接着画"时端点常常**重合** ✓，
    // 直接拼接会留下一个**零长度段** ✓（节点重复 ✓、渲染无害但数据脏 ✓，
    // 而且会让"接缝在哪"变得含糊 ✗）。规则：端点几乎重合（≤1e-6 ✓）时**丢掉第二个的首节点** ✓，
    // 也就是"在它们相接的地方连起来" ✓ —— 这是最符合直觉的读法 ✓。
    // **记录选择** ✓：设计没有规定 join/merge 如何处理重合端点 ✓；这里选择去重 ✓。
    let mut other_nodes = other_points;
    let deduped = {
        let same_point = match (points.last(), other_nodes.first()) {
            (Some(last), Some(first)) => {
                let (ax, ay) = if is_path {
                    node_xy(last)
                } else {
                    point_xy(last)
                };
                let (bx, by) = if is_path {
                    node_xy(first)
                } else {
                    point_xy(first)
                };
                (ax - bx).abs() <= 1e-6 && (ay - by).abs() <= 1e-6
            }
            _ => false,
        };
        if same_point && other_nodes.len() > 1 {
            other_nodes.remove(0);
            true
        } else {
            false
        }
    };
    let joined = points.len() + other_nodes.len();
    let seam = points.len();
    points.extend(other_nodes);
    if is_merge {
        // **`merge` 只对路径有意义** ✓：笔迹没有控制柄 ✓ ⇒ 若在这里静默等同于 `join`，
        // 调用方会以为"接缝平滑过了" ✗ —— 明确报错比静默降级诚实 ✓。
        if !is_path {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "merge 只作用于路径（笔迹没有控制柄，接缝无从平滑；请用 join）".to_owned(),
                ),
            ));
        }
        // **切线对齐（G1 连续）** ✓ —— 让接缝两侧的手柄各自**沿着相邻段的方向** ✓，
        // 而不是把手柄放在接缝弦上 ✗。
        //
        // 我第一版把手柄取成"弦长的 1/3" ✗ ⇒ 两个控制点都落在弦上 ⇒
        // 那条三次贝塞尔**恰好就是直线** ✓ ⇒ 与 `join` 的渲染**逐像素相同** ✓
        //（测试当场指出"两个名字应当有差别" ✗）。这是"看起来在做平滑、其实什么都没变"的典型 ✓：
        // 弦上的控制点只能表达直线 ✓，真正决定弯曲的是**切线方向** ✓。
        let (sx, sy) = node_xy(&points[seam - 1]);
        let (nx, ny) = node_xy(&points[seam]);
        let chord = ((nx - sx).powi(2) + (ny - sy).powi(2)).sqrt();
        let scale = chord / 3.0;
        // 进入接缝的方向：若 A 有前一个节点，取它指向接缝的方向 ✓（否则退化为弦方向 ✓）。
        let (px, py) = if seam >= 2 {
            node_xy(&points[seam - 2])
        } else {
            (sx - (nx - sx), sy - (ny - sy))
        };
        // 离开接缝的方向：若 B 有后续节点，取接缝指向它的方向 ✓。
        let (qx, qy) = if seam + 1 < points.len() {
            node_xy(&points[seam + 1])
        } else {
            (nx + (nx - sx), ny + (ny - sy))
        };
        let incoming = normalize_dir(sx - px, sy - py);
        let outgoing = normalize_dir(qx - nx, qy - ny);
        if let Some(map) = points[seam - 1].as_object_mut() {
            map.insert(
                "out".to_owned(),
                json!([incoming.0 * scale, incoming.1 * scale]),
            );
        }
        if let Some(map) = points[seam].as_object_mut() {
            map.insert(
                "in".to_owned(),
                json!([-outgoing.0 * scale, -outgoing.1 * scale]),
            );
        }
    }
    let mut data = data;
    // **按类型写回正确的键** ✓（路径是 `nodes` ✓、笔迹是 `points` ✓）——
    // 写错键的表现是"命令返回 ok 但什么都没变" ✗，正是本项目最忌的静默失效 ✓。
    let key = if is_path { "nodes" } else { "points" };
    data[key] = json!(points);
    // 两步归一个变更集 ✓（与 `detach_instance` 同一处理 ✓）。
    let changeset = yanshi_core::Changeset::new_id();
    let previous_changeset = ctx.changeset.replace(changeset.clone());
    let updated = ctx.commit(
        AtomKind::Supersede,
        json!({"object_id": object_id, "data": data}),
    );
    let removed = updated
        .as_ref()
        .ok()
        .map(|_| ctx.commit(AtomKind::Tombstone, json!({"object_id": other_id})));
    ctx.changeset = previous_changeset;
    let updated = updated?;
    let removed = removed.expect("接上成功后才 tombstone 第二条")?;
    Ok(
        // **`op` 如实回报** ✓（`join` 还是 `merge` ✓）+ **是否去过重** ✓
        //（端点重合时丢掉第二个的首节点 ✓ —— 调用方需要知道自己拿到的是几点 ✓）。
        json!({"ok": true, "op": op, "object_id": object_id, "joined_from": other_id,
              "points": joined, "seam_deduplicated": deduped, "changeset_id": changeset,
              "head": updated.head_seq.max(removed.head_seq)}),
    )
}

fn write_move_object(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let mut payload = json!({"object_id": object_id});
    if let Some(delta) = args.get("delta") {
        // **`delta` 是增量** ✓ —— 原样交给折叠层 **复合** ✓（只有它知道对象当前的变换 ✓）。
        // 此前这里把 dx/dy 编成**绝对矩阵** ✗ ⇒ 连续移动会**丢掉前面几步** ✓
        //（子 agent 实测：`dx:30` → 130 ✓，再 `dx:10` → **110** ✗，期望 140 ✓）。
        let dx = delta.get("dx").and_then(Value::as_f64).unwrap_or(0.0);
        let dy = delta.get("dy").and_then(Value::as_f64).unwrap_or(0.0);
        payload["delta"] = json!({"dx": dx, "dy": dy});
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
/// 选区语义（设计 4.4）：用户确认采用**路线 A「约束落笔」** ✓ ——
/// 选区只约束**之后新落笔**的像素 ✓，不改写已有内容 ✓，删掉选区后已画内容**保持不变** ✓
/// （因为约束是在渲染期按日志重新计算的 ✓）。设计只给了数据模型与类型清单、
/// 未规定其对落笔的作用，这一语义是用户拍板的，已记入 implementation-notes ✓。
fn write_create_selection(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let selection_id = require_str(args, "selection_id")?;
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
        "selection_id": selection_id,
        "shape": shape,
        "feather": feather,
        "mode": mode,
        "invert": args.get("invert").and_then(Value::as_bool).unwrap_or(false),
    });
    if let Some(linked) = optional_str(args, "linked_layer") {
        payload["linked_layer"] = json!(linked);
    }
    let result = ctx.commit(AtomKind::CreateSelection, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn write_delete_selection(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let selection_id = require_str(args, "selection_id")?;
    let result = ctx.commit(AtomKind::Tombstone, json!({"selection_id": selection_id}))?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

fn read_list_selections(ctx: &mut ToolContext<'_>) -> Result<Value> {
    let state = document_state(ctx)?;
    let selections: Vec<Value> = state
        .selections
        .values()
        .filter(|selection| !selection.is_deleted())
        .map(|selection| {
            json!({
                "selection_id": selection.id,
                "shape": selection.shape,
                "feather": selection.feather,
                "mode": selection.mode,
                "invert": selection.invert,
                "linked_layer": selection.linked_layer,
                "refined_edges": selection.refined_edges,
            })
        })
        .collect();
    Ok(json!({"selections": selections, "count": selections.len()}))
}

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
    let annotation_id = optional_str(args, "annotation_id");
    let result = ctx.commit(
        AtomKind::Suggest,
        json!({
            "patch": patch,
            "annotation_id": annotation_id,
            "summary": optional_str(args, "summary"),
            "priority": priority,
        }),
    )?;
    // 回写标注的 `suggestion_id`，让"标注 → 建议"成为**双向**可查的链接 ✓。
    // 关联的标注必须存在：否则会留下一条悬空引用（`list_annotations{suggestion_id}` 查不到任何东西 ✓）。
    let mut annotation_linked = false;
    if let Some(annotation_id) = annotation_id.as_deref() {
        let now = ctx.now;
        let document = ctx.workspace.document_mut(&ctx.doc_id)?;
        document
            .annotations_mut()
            .link_suggestion(annotation_id, &result.atom_id, now)?;
        annotation_linked = true;
    }
    Ok(json!({
        "suggestion_id": result.atom_id,
        "seq": result.seq,
        "steps": patch.len(),
        "priority": priority,
        "annotation_id": annotation_id,
        "annotation_linked": annotation_linked,
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

/// **原子类型的统一拼法** ✓ —— 返回 **snake_case** ✓（`create_layer` ✓、`comment` ✓），
/// **与 `get_log` 的序列化结果一致** ✓。
///
/// **为什么要有它** ✓（本轮修的真问题 ✓）：同一个概念此前有**两种拼法** ✗ ——
/// `get_log` 直接序列化枚举 ✓ ⇒ `create_layer` ✓；而 `get_changesets` 用
/// `format!("{:?}", kind).to_lowercase()` ✗ ⇒ `createlayer` ✗（**下划线没了** ✗）；
/// 我上一轮加的 `get_atom` 也用了后者 ✗ ⇒ `Comment` ✗。
/// ⇒ **同一个值在不同工具里拼法不同** ✓ 会让调用方**没法可靠地比较** ✓ —— 这是**产品缺陷** ✓，
/// 不是测试写错 ✓（测试只是把它照出来了 ✓）。
fn kind_label(kind: &AtomKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{kind:?}"))
}

/// **`new_document`** ✓ —— 一键得到一块**空白画布** ✓（真实用户 §六-4 ✓）。
///
/// **用户的原话** ✓："同一 `--root` 下重复打开页面会载入同一文档，三次绘画叠在一张画布上；
/// 自动化测试/创作时必须手动删 `--root` 目录或点新建，**没有 `new_document` 的 MCP 工具**" ✓。
/// **为什么值得** ✓：**文档 id 就是持久单元** ✓（这是设计 ✓，不是缺陷 ✓）
/// ⇒ 想要一块干净画布 ✓ 就得显式新建 ✓ ⇒ 那就给它一个明确的入口 ✓，而不是让人去删目录 ✗。
///
/// **与"文档隔离"的关系** ✓：本工具**不改变** id 语义 ✓ —— 它只是把"新建"这件事
/// 从"手工删目录"变成"一次调用" ✓。想彼此隔离就**用不同的 doc_id** ✓（默认值已由 `--doc` 提供 ✓）。
fn write_new_document(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let doc_id = optional_str(args, "doc_id").unwrap_or_else(|| ctx.doc_id.clone());
    // 尺寸：缺了就给一句**能照做**的错 ✓（本轮刚给"不存在"加过可用值 ✓，同一个道理 ✓）。
    let width = optional_u64(args, "width")
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("new_document 需要 width ✓（例如 1024）"),
            )
        })?
        .clamp(1, 65536) as u32;
    let height = optional_u64(args, "height")
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("new_document 需要 height ✓（例如 1024）"),
            )
        })?
        .clamp(1, 65536) as u32;
    let background = match args.get("background") {
        Some(value) if !value.is_null() => {
            let channel = |name: &str, fallback: i64| {
                value
                    .get(name)
                    .and_then(Value::as_i64)
                    .unwrap_or(fallback)
                    .clamp(0, 255)
            };
            json!({
                "r": channel("r", 255),
                "g": channel("g", 255),
                "b": channel("b", 255),
                "a": channel("a", 255),
            })
        }
        _ => json!({"r": 255, "g": 255, "b": 255, "a": 255}),
    };
    // **重建** ✓：同 id 已有文档时**覆盖**它 ✓ ⇒ 语义是"给我一块干净画布" ✓（用户的诉求正是这个 ✓）。
    let mut request = crate::document::NewDocument::new(&doc_id, width, height);
    request.background = background;
    // **"已经有同 id 的文档"要给出下一步** ✓（本轮刚给"不存在"做过同样的事 ✓）：
    // 底层那句只写"文档 X 已打开" ✗ ⇒ 调用方还是不知道该干什么 ✓
    // ⇒ 这里补上**唯一正确的做法**：**换一个 doc_id** ✓
    //（文档 id 就是持久单元 ✓ —— 这是设计 ✓，所以"新画布"= 新 id ✓）。
    ctx.workspace
        .create_document(request, &ctx.actor, &ctx.session)
        .map_err(|error| {
            let detail = error.context.detail.clone().unwrap_or_default();
            if detail.contains("已打开") {
                YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!(
                        "文档 {doc_id} 已经打开 ⇒ 换一个 doc_id 就是一块新画布（例如 {doc_id}-2）；\
                         本工具不会清空已存在的文档"
                    )),
                )
                .with_atom(error.context.atom_id.clone().unwrap_or_default())
            } else {
                error
            }
        })?;
    Ok(json!({
        "doc_id": doc_id,
        "width": width,
        "height": height,
        "blank": true,
    }))
}

/// **`medium_stroke`** ✓ —— 用**介质插件**画一笔 ✓（真实用户 P1-3 要的那件事 ✓）。
///
/// **它解决什么** ✓：油画/水彩这些质感原本**只有浏览器端能画** ✗
///（插件是 wasm ✓，由浏览器各自实例化 ✓）⇒ MCP/agent 侧**完全用不上** ✓，
/// 只能画出"均匀的面条线" ✗ —— 用户报告里"不够真实、艺术感不够"的根因之一 ✓。
///
/// **为什么服务端现在能画** ✓（两步 ✓）：
/// 1. 插件是**纯 Rust 零依赖** ✓ ⇒ 加 `rlib` 就能当库链进来 ✓（工作区内部依赖 ✓，不引外部依赖 ✓）；
/// 2. 它们原本**导出同名 C 符号** ✗ ⇒ 六个一起链会重复符号 ✗
///    ⇒ 给导出名做了**平台分叉** ✓：wasm 照旧 `yanshi_dab` ✓（**已发布 ABI 不变** ✓），
///    原生用 `yanshi_oil_dab` 等**唯一名** ✓ ⇒ **六个能同时链进同一个二进制** ✓。
///
/// **产出与浏览器端同形** ✓：`paint_stroke` 给出一块**直通 RGBA8** ✓ ⇒ 走 blob 先行 ✓
/// 再用**同一个** `import_image` 提交 ✓（`bitmap` + `medium` 描述符 ✓）
/// ⇒ 渲染路径、回放、撤销、以及"**插件 id + version 随对象记录**"全都自动一致 ✓
///（升级插件不会悄悄改变旧文档 ✓ —— 设计 11.1 的硬要求 ✓）。
fn write_medium_stroke(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let medium = require_str(args, "medium")?;
    let raw_points = args
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("points 必须是数组：[[x,y], [x,y,pressure?], ...]"),
            )
        })?;
    let mut points = Vec::with_capacity(raw_points.len());
    for (index, item) in raw_points.iter().enumerate() {
        let pair = item.as_array().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "第 {index} 个点不是数组 ⇒ 应形如 [x, y] 或 [x, y, pressure]"
                )),
            )
        })?;
        let x = pair.first().and_then(Value::as_f64).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("第 {index} 个点缺 x")),
            )
        })?;
        let y = pair.get(1).and_then(Value::as_f64).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("第 {index} 个点缺 y")),
            )
        })?;
        let pressure = pair.get(2).and_then(Value::as_f64).unwrap_or(1.0);
        points.push((x, y, pressure));
    }
    let size = args.get("size").and_then(Value::as_f64).unwrap_or(24.0);
    let load = args.get("load").and_then(Value::as_f64).unwrap_or(1.0);
    let wetness = args.get("wetness").and_then(Value::as_f64).unwrap_or(0.4);
    // **纹理强度** ✓（真实用户报告"油画纹理过重"✓）：缺省 **0.0** ✓
    // ⇒ 与"这个参数根本不存在时"**逐字节相同** ✓ ⇒ **旧调用方的输出一个像素都不变** ✓。
    let texture = args.get("texture").and_then(Value::as_f64).unwrap_or(0.0);
    // 色按 0..255 收 ✓ ⇒ 转成插件要的 0..1 ✓（缺省不透明黑 ✓）。
    let color = match args.get("color") {
        Some(value) if !value.is_null() => {
            let channel = |name: &str, fallback: f32| {
                value
                    .get(name)
                    .and_then(Value::as_f64)
                    .map(|v| (v / 255.0) as f32)
                    .unwrap_or(fallback)
            };
            [
                channel("r", 0.0),
                channel("g", 0.0),
                channel("b", 0.0),
                channel("a", 1.0),
            ]
        }
        _ => [0.0, 0.0, 0.0, 1.0],
    };
    // **先按同一块区域把画布渲染出来** ✓ ⇒ 交给插件当"笔下的颜色" ✓
    //（用户报告第 2 条 ✓：此前是"静态透明度叠加" ✗，没有掠过底色的取样与拖曳混色 ✓）。
    //
    // **为什么先 `plan_region`** ✓：区域必须**先于绘制可知** ✓，且必须与真正落笔用的是
    // **同一份实现** ✗（各算一次必然漂移 ⇒ 采样点与落笔点错位 ✓）。
    // **拿不到底色也不阻断** ✓：照旧画 ✓（退化为旧语义 ✓）——
    // 介质本来就是 D2 ✓，不该因为"读不到底色"就整笔失败 ✗。
    let (planned, _, _) = yanshi_medium_host::plan_region(&points, size)?;
    // **画布尺寸要随结果一起回去** ✓（真实用户画作的**头号原因** ✓）。
    //
    // **他的原子实测** ✓：`region {x: -45, y: -45, w: 1114, h: 100}` ✓ —— 而画布是 **1024×1024** ✓
    // ⇒ 笔触**跑到画布外面** ✓、画面**四边被裁** ✓、中间大片空白 ✓ ⇒ 他说"一塌糊涂" ✓。
    // **工具此前从不告诉他画布多大** ✗ ⇒ 调用方只能**猜** ✓（猜错就整幅画错位 ✓）。
    // ⇒ 从这里开始 ✓：**每次介质笔触的结果都带上画布尺寸** ✓，
    // 并且**大部分落在画布外时明确警告** ✓ —— 与"错误里带可用选项"同一套思路 ✓：
    // **别让人画完才发现画错地方** ✗。
    let (canvas_width, canvas_height) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| {
            (
                document.state().width as f64,
                document.state().height as f64,
            )
        })
        .unwrap_or((0.0, 0.0));
    let inside = |span: (f64, f64), limit: f64| -> f64 {
        let (start, length) = span;
        (start + length).min(limit).max(start).min(limit) - start.max(0.0)
    };
    let inside_area = if canvas_width > 0.0 && canvas_height > 0.0 {
        inside((planned.x, planned.w), canvas_width) * inside((planned.y, planned.h), canvas_height)
    } else {
        planned.w * planned.h
    };
    let outside_fraction = if planned.w * planned.h > 0.0 {
        (1.0 - inside_area / (planned.w * planned.h)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let base = ctx
        .workspace
        .render_region_raw(&ctx.doc_id, planned)
        .ok()
        .map(|(width, height, pixels)| {
            // 尺寸理应与规划一致 ✓；不一致就**不用**它 ✓（宁可退化 ✓，不越界 ✗）。
            if width as usize == planned.w as usize && height as usize == planned.h as usize {
                pixels
            } else {
                Vec::new()
            }
        })
        .filter(|pixels| !pixels.is_empty());

    let (region, rgba) = yanshi_medium_host::paint_stroke_over(
        &medium,
        &points,
        yanshi_medium_host::StrokeSettings {
            size,
            color,
            load,
            wetness,
            texture,
        },
        base.as_deref(),
    )?;
    // **blob 先行** ✓：先把像素写进存储 ✓，再提交引用它的原子 ✓（与 `import_psd` 同一套路 ✓）。
    let composite = ctx.workspace.store().put(&rgba)?;
    let spec = yanshi_medium_host::spec(&medium).expect("paint_stroke 已经校验过介质名");
    let object_id = optional_str(args, "object_id");
    let mut import_args = json!({
        "layer_id": layer_id,
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
        // **插件 id + version 随对象记录** ✓（设计 11.1 ✓）。
        "medium": {"id": spec.id, "version": spec.version},
    });
    if let Some(object_id) = object_id {
        import_args["object_id"] = json!(object_id);
    }
    let mut value = write_import_image(ctx, &import_args)?;
    value["medium"] = json!({"id": spec.id, "version": spec.version});
    value["dabs"] = json!(points.len());
    // **画布尺寸每次都回** ✓（调用方据此对齐坐标 ✓）。
    value["canvas"] = json!({"width": canvas_width, "height": canvas_height});
    value["outside_fraction"] = json!((outside_fraction * 1000.0).round() / 1000.0);
    if outside_fraction > 0.5 {
        // **过半落在画布外 ⇒ 明确说出来** ✓（这正是那四幅画的病根 ✓）。
        value["coordinate_warning"] = json!(format!(
            "这一笔有 {:.0}% 落在画布之外（画布 {}×{}，这笔的范围 x={} y={} w={} h={}）\
             ⇒ 多半是**坐标空间不一致**：请按画布尺寸给坐标，或先取一次 get_document 确认尺寸",
            outside_fraction * 100.0,
            canvas_width,
            canvas_height,
            planned.x,
            planned.y,
            planned.w,
            planned.h,
        ));
    }
    Ok(value)
}

/// **`export_project`** ✓ —— 一键打成 `.yanshi` 工程包 ✓（真实用户提的缺口 ✓）。
///
/// **用户原话** ✓："目前仅支持 `export_png` ✓，缺少一键将 `atoms.jsonl`、`meta.json`
/// 与 CAS blobs 归档为 `.yanshi` 工程文件的内置命令" ✓。
///
/// **它顺手解决了另一件事** ✓：用户此前**手工 zip** 文档目录 ✓ ⇒ 把磁盘上那份**过期的 `render.png` 缓存**
/// 一起装了进去 ✓ ⇒ 四个工程包的预览**全是空白** ✗（实测缓存 `render.seq` 5–6 ✓ vs 原子 97–217 ✓）。
/// ⇒ 本工具**当场渲染一张最新的** ✓ 写进包 ✓ ⇒ **从源头**不再有这个问题 ✓。
///
/// **格式** ✓：未压缩 **tar** ✓（零依赖手写 ✓，`tar -tf/-xOf` 就能看 ✓）。
/// 不压缩的取舍写在 `archive.rs` 里 ✓（工程包主体是 PNG 与原始像素 ✓，本来也压不动 ✓）。
/// **列出纹理缓存** ✓（只读 ✓）。
///
/// **为什么在工具层** ✓（用户新加的硬要求 ✓）：**MCP 与 Web 都要能用** ✓ ——
/// 能力放在工具层 ✓ ⇒ 两边**自动同时获得** ✓；若只做在查看器里 ✗，MCP 就用不上 ✗ ✓。
/// **列出某类资产** ✓（只读 ✓）—— **MCP 与 Web 共用同一个入口** ✓（用户那条硬要求 ✓）。
/// **用 `.myb` 笔刷画一笔** ✓（Hokusai 引擎 ✓）。
///
/// **为什么照 `medium_stroke` 的样子写** ✓：那条路已经跑通并被用户用过 ✓ ——
/// **先 `store().put(rgba)` 得到 blob ✓，再复用 `import_image` 提交引用它的原子** ✓。
/// 两条引擎**共用同一个提交路径** ✓ ⇒ 提交语义（区域、blob 先行、对象 id ✓）**只有一份** ✓。
///
/// **补间必须自己做** ✗：Hokusai 的 `stroke_to` 要的是**连续的指针流** ✓
/// ⇒ 只喂稀疏控制点会得到**离散盖章** ✗ —— 用户正是在介质那边报过这个 ✓
/// ⇒ 这里按 **2px** 步长细分 ✓（比笔尖细得多 ✓），压力沿段线性插值 ✓。
/// **读一个调色板的颜色** ✓（只读 ✓）—— **MCP 与 Web 共用** ✓（用户硬要求 ✓）。
///
/// **截断必须在响应里说清** ✗：用户导入的调色板可能上千色 ✓
/// ⇒ 只给前 512 个却不说 ✓ ⇒ 调用方会以为"这个板就这么多" ✗
///（"说了一半、不说另一半"和"接受了却没用"是同一类病 ✓）。
/// **把一张纹理铺成背景** ✓（目标的第 ② 件 ✓）。
///
/// **三种铺法** ✓：`tile`（**默认** ✓ —— 纸张/画布本来就是可平铺的 ✓，而且**不插值** ⇒ 最保真 ✓）、
/// `stretch`（拉伸到整幅 ✓）、`cover`（**等比**放大到铺满再居中裁 ✓ —— 不会把纸纹压扁 ✓）。
///
/// **为什么默认 tile** ✓：这批纹理是 ambientCG 的**无缝**纸张/纸板 ✓ ⇒ 平铺既省内存 ✓
/// 又**不引入重采样** ✓（缩放会把纸纹糊掉 ✗）。`stretch`/`cover` 复用我们已有的
/// `yanshi_core::resample` ✓（双线性 ✓）—— **不引新依赖** ✓。
///
/// **落到底部** ✓：背景就该在最下面 ✓ ⇒ 新建图层后用 `reorder_layers` 把它放到**最底层** ✓
///（`create_layer` 会把新层放在最上面 ✗ —— 这一点与用户报过的"顺序"问题同源 ✓）。
/// **渐变填充** ✓（目标 (d) ✓）—— 大面积底色的正路 ✓。
///
/// **为什么它比"用大笔刷铺"稳** ✓（用户报过"大面积背景难处理" ✓）：
/// 笔刷铺底会留下**笔触边缘**与**采样噪声** ✗（他实测到的"横向条带"与"盖章圆点" ✓）；
/// 而渐变是**纯函数** ✓：给定两端色与方向 ✓ ⇒ 逐像素可复现 ✓、没有随机、没有重叠 ✗ ✓。
///
/// **插值空间的选择** ✓（记录选择 ✓）：在 **sRGB 分量上直接线性插值** ✓ ——
/// 这与多数图形软件的"默认渐变"一致 ✓、也与本项目的像素语义一致 ✓；
/// **物理上更"对"的是线性光空间** ✗ ⇒ 那会明显改变中间的亮度 ✓
/// ⇒ 留作**将来的可选参数** ✓，不在这里偷偷做掉 ✓（"悄悄改了效果"比"没做"更糟 ✗）。
/// **导入 `.yanshi` 工程包** ✓（与 `export_project` 配对 ✓）。
///
/// **放在工具层** ✓ ⇒ **MCP 与 Web 都能用** ✓（用户那条硬要求 ✓）——
/// 导出那边一直是这样 ✓，导入这边现在就补上 ✓。
fn write_import_project(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let path = require_str(args, "path")?;
    let doc_id = optional_str(args, "doc_id");
    let bytes = std::fs::read(&path).map_err(|error| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("读不到工程包 {path}：{error}")),
        )
    })?;
    let value = ctx.workspace.import_project(&bytes, doc_id.as_deref())?;
    Ok(value)
}

fn write_gradient_fill(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let from = parse_color_arg(args.get("from"), "from")?;
    let to = parse_color_arg(args.get("to"), "to")?;
    let kind = optional_str(args, "kind").unwrap_or_else(|| "linear".to_string());
    if !matches!(kind.as_str(), "linear" | "radial") {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("未知渐变类型 {kind} ⇒ 可用：linear / radial")),
        ));
    }
    let (canvas_width, canvas_height) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| (document.state().width, document.state().height))
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("找不到文档 {}", ctx.doc_id)),
            )
        })?;
    // **区域** ✓：不给就整幅 ✓（画布尺寸 ✓）。
    let (target_x, target_y, width, height) = match args.get("region") {
        Some(value) => {
            let parsed = parse_bbox(value)?;
            let x = (parsed.x.max(0.0) as i32).min(canvas_width as i32);
            let y = (parsed.y.max(0.0) as i32).min(canvas_height as i32);
            let w = (parsed.w.max(1.0) as i32)
                .min(canvas_width as i32 - x)
                .max(1) as usize;
            let h = (parsed.h.max(1.0) as i32)
                .min(canvas_height as i32 - y)
                .max(1) as usize;
            (x, y, w, h)
        }
        None => (0, 0, canvas_width as usize, canvas_height as usize),
    };
    if width == 0 || height == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("文档尺寸是 0 ⇒ 填不了渐变"),
        ));
    }

    let angle_degrees = args.get("angle").and_then(Value::as_f64).unwrap_or(0.0);
    let angle_radians = angle_degrees.to_radians();
    // **方向向量** ✓：角度按屏幕坐标量 ✓（x 向右 ✓、y 向下 ✓）⇒ 90° 就是"从上到下" ✓。
    let direction = (angle_radians.cos(), angle_radians.sin());
    // **线性渐变的投影范围** ✓：把矩形四角投到方向上 ✓ 取最小与最大 ✓
    // ⇒ 这样**任意角度**下渐变都能正好铺满该区域 ✓（而不是只铺一半 ✗）。
    let corners = [
        (0.0, 0.0),
        (width as f64, 0.0),
        (0.0, height as f64),
        (width as f64, height as f64),
    ];
    let projections: Vec<f64> = corners
        .iter()
        .map(|(x, y)| x * direction.0 + y * direction.1)
        .collect();
    let min_projection = projections.iter().cloned().fold(f64::MAX, f64::min);
    let max_projection = projections.iter().cloned().fold(f64::MIN, f64::max);
    let span = (max_projection - min_projection).max(1e-6);

    let center = match args.get("center") {
        Some(value) => {
            // **与 `parse_bbox` 同样宽容** ✓：`{x,y}` 与 `[x,y]` 都收 ✓ ——
            // 同一个工具里两种点/框写法**要么都收、要么都不收** ✗（只收一种 ⇒ 调用方要记两套 ✓）。
            let x = value.get("x").and_then(Value::as_f64).or_else(|| {
                value
                    .as_array()
                    .and_then(|items| items.first())
                    .and_then(Value::as_f64)
            });
            let y = value.get("y").and_then(Value::as_f64).or_else(|| {
                value
                    .as_array()
                    .and_then(|items| items.get(1))
                    .and_then(Value::as_f64)
            });
            match (x, y) {
                // **转成区域内的相对坐标** ✓（渐变按区域算 ✓）。
                (Some(x), Some(y)) => (x - target_x as f64, y - target_y as f64),
                _ => {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("center 必须是 {x,y} 或 [x,y]"),
                    ))
                }
            }
        }
        None => (width as f64 / 2.0, height as f64 / 2.0),
    };
    let radius = args
        .get("radius")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| ((width as f64).powi(2) + (height as f64).powi(2)).sqrt() / 2.0)
        .max(1e-6);

    let mut rgba = vec![0u8; width * height * 4];
    for row in 0..height {
        for column in 0..width {
            // **线性**：沿方向投影 ⇒ 归一化到 0..1 ✓
            // **径向**：到中心的距离 ⇒ 除以半径 ✓（超出就夹到 1 ✓）。
            let t = if kind == "radial" {
                let dx = column as f64 - center.0;
                let dy = row as f64 - center.1;
                ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0)
            } else {
                let projection = column as f64 * direction.0 + row as f64 * direction.1;
                ((projection - min_projection) / span).clamp(0.0, 1.0)
            };
            let at = (row * width + column) * 4;
            for channel in 0..4 {
                let a = from[channel] as f64;
                let b = to[channel] as f64;
                rgba[at + channel] = (a + (b - a) * t).round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    // **blob 先行** ✓，与介质 / 笔刷 / 纹理三条路完全同源 ✓。
    let composite = ctx.workspace.store().put(&rgba)?;
    let import_args = json!({
        "layer_id": layer_id,
        "object_id": optional_str(args, "object_id"),
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": target_x, "y": target_y, "w": width, "h": height},
    });
    let mut value = write_import_image(ctx, &import_args)?;
    value["kind"] = json!(kind);
    value["angle"] = json!(angle_degrees);
    value["region"] = json!({"x": target_x, "y": target_y, "w": width, "h": height});
    Ok(value)
}

/// **解析 `{r,g,b,a}` 颜色参数** ✓（0..255 ✓；`a` 缺省 255 ✓）。
fn parse_color_arg(value: Option<&Value>, name: &str) -> Result<[u8; 4]> {
    let object = value.and_then(Value::as_object).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{name} 必须是对象 {{r,g,b,a}}（0..255）")),
        )
    })?;
    let mut out = [0u8; 4];
    for (index, key) in ["r", "g", "b", "a"].iter().enumerate() {
        let raw = object.get(*key).and_then(Value::as_u64);
        let fallback = if *key == "a" { 255 } else { 0 };
        let channel = raw.unwrap_or(fallback).min(255) as u8;
        out[index] = channel;
    }
    Ok(out)
}

fn write_texture_background(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let texture = require_str(args, "texture")?;
    let mode = optional_str(args, "mode").unwrap_or_else(|| "tile".to_string());
    if !matches!(mode.as_str(), "tile" | "stretch" | "cover") {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("未知铺法 {mode} ⇒ 可用：tile / stretch / cover")),
        ));
    }
    // **可选区域** ✓：给了就只铺那一块 ✓（这是"纹理补丁"与"整幅背景"的**唯一区别** ✓）。
    // **裁剪到画布内** ✓ —— 区域超出画布没有任何意义 ✓，而且会白白生成一堆像素 ✗。
    let region = match args.get("region") {
        Some(value) => {
            let parsed = parse_bbox(value)?;
            Some((
                parsed.x.max(0.0) as i32,
                parsed.y.max(0.0) as i32,
                parsed.w.max(1.0) as i32,
                parsed.h.max(1.0) as i32,
            ))
        }
        None => None,
    };
    let (canvas_width, canvas_height) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| (document.state().width, document.state().height))
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("找不到文档 {}", ctx.doc_id)),
            )
        })?;
    // **目标尺寸** ✓：给了区域就是区域大小 ✓，否则整幅 ✓；
    // 还要**夹在画布内** ✓（区域可以超出去 ✓，但我们只铺真正落在画布上的部分 ✓）。
    let (target_x, target_y, width, height) = match region {
        Some((rx, ry, rw, rh)) => {
            let x = rx.min(canvas_width as i32).max(0);
            let y = ry.min(canvas_height as i32).max(0);
            let w = rw.min(canvas_width as i32 - x).max(1) as usize;
            let h = rh.min(canvas_height as i32 - y).max(1) as usize;
            (x, y, w, h)
        }
        None => (0, 0, canvas_width as usize, canvas_height as usize),
    };
    if width == 0 || height == 0 || canvas_width == 0 || canvas_height == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("文档尺寸是 0 ⇒ 铺不了背景"),
        ));
    }

    // **纹理同样走"缓存优先、内置其次"** ✓（与列举一致 ✓）。
    let path = ctx.workspace.resolve_asset("texture", &texture)?;
    let bytes = std::fs::read(&path).map_err(|error| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("读不到纹理 {}：{error}", path.display())),
        )
    })?;
    let Some((tile_width, tile_height, tile_rgba)) = yanshi_render::png::decode_png(&bytes) else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "{} 不是能解码的 PNG ⇒ 本项目只解 8 位、非隔行的 RGB/RGBA ✓",
                path.display()
            )),
        ));
    };
    let (tile_width, tile_height) = (tile_width as usize, tile_height as usize);
    if tile_width == 0 || tile_height == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{} 的尺寸是 0", path.display())),
        ));
    }

    let mut rgba = vec![0u8; width * height * 4];
    match mode.as_str() {
        "stretch" => {
            // **真实签名是 u32 + 返回 Option** ✗（我第一版按 usize 写 ✓ ⇒ 编译器当场指出 ✓）。
            rgba = yanshi_core::resample::resample_rgba(
                &tile_rgba,
                tile_width as u32,
                tile_height as u32,
                width as u32,
                height as u32,
                yanshi_core::resample::ResampleFilter::Bilinear,
            )
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail("把纹理拉伸到画布尺寸时重采样失败"),
                )
            })?;
        }
        "cover" => {
            // **等比放大到铺满，再居中裁** ✓ —— 取两个方向的较大倍率 ✓。
            let scale = (width as f64 / tile_width as f64).max(height as f64 / tile_height as f64);
            let scaled_width = ((tile_width as f64 * scale).round() as usize).max(1);
            let scaled_height = ((tile_height as f64 * scale).round() as usize).max(1);
            let scaled = yanshi_core::resample::resample_rgba(
                &tile_rgba,
                tile_width as u32,
                tile_height as u32,
                scaled_width as u32,
                scaled_height as u32,
                yanshi_core::resample::ResampleFilter::Bilinear,
            )
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail("把纹理等比放大时重采样失败"),
                )
            })?;
            let offset_x = (scaled_width.saturating_sub(width)) / 2;
            let offset_y = (scaled_height.saturating_sub(height)) / 2;
            for row in 0..height {
                for column in 0..width {
                    let source_x = (offset_x + column).min(scaled_width - 1);
                    let source_y = (offset_y + row).min(scaled_height - 1);
                    let from = (source_y * scaled_width + source_x) * 4;
                    let to = (row * width + column) * 4;
                    rgba[to..to + 4].copy_from_slice(&scaled[from..from + 4]);
                }
            }
        }
        _ => {
            // **tile**：逐像素取模 ✓ —— 纹理是无缝的 ✓ ⇒ 不重采样 ⇒ 最保真 ✓。
            // **按文档坐标取模** ✓（而不是按区域内的相对坐标 ✓）⇒
            // 先铺一块补丁、再铺整幅背景 ⇒ 花纹是**对齐的** ✓（否则会看出接缝 ✗）。
            for row in 0..height {
                for column in 0..width {
                    let document_x = target_x as usize + column;
                    let document_y = target_y as usize + row;
                    let source_x = document_x % tile_width;
                    let source_y = document_y % tile_height;
                    let from = (source_y * tile_width + source_x) * 4;
                    let to = (row * width + column) * 4;
                    rgba[to..to + 4].copy_from_slice(&tile_rgba[from..from + 4]);
                }
            }
        }
    }

    // **图层** ✓：没指定就新建一个，并把它**沉到最底** ✓。
    let mut created_layer: Option<String> = None;
    let layer_id = match optional_str(args, "layer_id") {
        Some(given) => given,
        None => {
            let made = write_create_layer(ctx, &json!({ "name": "背景（纹理）" }))?;
            let new_id = made
                .get("layer_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    YanshiError::new(
                        ErrorCode::PreconditionFailed,
                        ErrorContext::detail(format!("建背景图层之后没拿到 layer_id：{made}")),
                    )
                })?
                .to_string();
            created_layer = Some(new_id.clone());
            // **沉到底** ✓：`reorder_layers` 收的是**自下而上**的顺序 ✓
            //（我实测过 ✓：传 [second, first] 顺序真的会变 ✓）⇒ 把新层放最前 ✓。
            let listed = read_list_layers(ctx)?;
            let mut order: Vec<String> = vec![new_id.clone()];
            if let Some(items) = listed.get("layers").and_then(Value::as_array) {
                for item in items {
                    if let Some(id) = item.get("layer_id").and_then(Value::as_str) {
                        if id != new_id {
                            order.push(id.to_string());
                        }
                    }
                }
            }
            // **只有整幅背景才沉到底** ✓；给了 `region` 是**补丁** ✓ ⇒ 不重排 ✓
            //（把一块补丁强行沉到最底 ⇒ 一定会盖错东西 ✗）。
            if region.is_none() {
                write_reorder_layers(ctx, &json!({ "order": order }))?;
            }
            new_id
        }
    };

    // **像素照样"blob 先行"** ✓，与介质/笔刷两条路完全同源 ✓。
    let composite = ctx.workspace.store().put(&rgba)?;
    let import_args = json!({
        "layer_id": layer_id,
        "object_id": optional_str(args, "object_id"),
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        // **请求里只放 `import_image` 认的键** ✗（`is_patch` 是我们自己的语义 ✓
        // ⇒ 它属于**响应** ✓；塞进请求只会让人以为下游会用它 ✓）。
        "region": {"x": target_x, "y": target_y, "w": width, "h": height},
    });
    // **指定了非空图层时要提醒一句** ✗（真实观察 ✓）：实测发现
    // **同一个图层里，后导入的位图会排在已有对象之上** ✗
    //（我用"背景 + 同层画红"验过 ✓：形状对象还在 ✓ 但被背景盖住了 ✓）
    // ⇒ "背景要沉到最底"这件事**只能靠图层** ✓ ⇒ 缺省已经自建并沉底 ✓；
    // 若调用方**自己指定**了一个**已经有内容**的图层 ✓，得让它知道会发生什么 ✓。
    let layer_had_content = {
        let listed = read_list_layers(ctx)?;
        listed
            .get("layers")
            .and_then(Value::as_array)
            .and_then(|layers| {
                layers.iter().find(|layer| {
                    layer.get("layer_id").and_then(Value::as_str) == Some(layer_id.as_str())
                })
            })
            // **字段名与类型都要照真实的来** ✗ —— 这里我错了两次 ✓，两次都是测试抓出来的 ✓：
            // 第一次按 `object_count` 写 ✓ ⇒ **字段名不存在** ✗；
            // 第二次按"`objects` 数组"写 ✓ ⇒ **类型不对** ✗。
            // **实测** ✓：`list_layers` 里 `objects` 是**整数**（对象个数 ✓，如 `2`）。
            // ⇒ 两次都会让 `unwrap_or(...)` 兜底 ⇒ **警告永远不会触发** ✗ ✓
            //（"加了却永远不生效" 与"接受了却没用"是同一类病 ✓）。
            .and_then(|layer| layer.get("objects").and_then(Value::as_u64))
            .map(|count| count > 0)
            .unwrap_or(false)
    };
    let mut value = write_import_image(ctx, &import_args)?;
    value["texture"] = json!(texture);
    value["mode"] = json!(mode);
    if created_layer.is_none() && layer_had_content {
        value["warning"] = json!(
            "指定的图层里本来就有对象 ⇒ 实测**同层内后导入的位图会压在已有对象之上** \
             ⇒ 想让纹理当背景，请**让它单独占一层**（不传 layer_id 就会自建一层并沉到最底）"
        );
    }
    // **`region` 要显式回** ✓ —— `import_image` 回的是 `dirty_bbox` ✓，
    // 而本项目其它落笔工具（`medium_stroke` / `brush_stroke` ✓）都回 `region` ✓
    // ⇒ **同一种语义在三个工具里要用同一个名字** ✗（各叫各的 ⇒ 调用方要记三套 ✓）。
    // **起点要用真实的** ✗ —— 我第一版写死了 `0,0` ✓ ⇒ 补丁的响应会显示"从原点开始" ✗
    //（而 `dirty_bbox` 明明是对的 ✓ ⇒ 两个字段互相矛盾 ✓，调用方只能猜 ✓）。
    value["region"] = json!({"x": target_x, "y": target_y, "w": width, "h": height});
    // **说清这次是"背景"还是"补丁"** ✓ —— 两种用法**副作用不同** ✓
    //（补丁**不会**被沉到最底 ✗）⇒ 不说清的话调用方会误判图层顺序 ✓。
    value["is_patch"] = json!(region.is_some());
    value["tile_size"] = json!({"width": tile_width, "height": tile_height});
    if let Some(layer) = created_layer {
        value["created_layer"] = json!(layer);
    }
    Ok(value)
}

fn write_list_palette_colors(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let palette = require_str(args, "palette")?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(512) as usize;
    let all = ctx.workspace.palette_colors(&palette)?;
    let total = all.len();
    let take = if limit == 0 { total } else { limit.min(total) };
    let colors: Vec<Value> = all
        .iter()
        .take(take)
        .map(|color| {
            json!({
                "r": color.r, "g": color.g, "b": color.b,
                // **同时给十六进制** ✓：界面要直接写进 `<input type=color>` ✓，
                // 而调用方多半不想自己格式化 ✓。
                "hex": format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b),
                "name": color.name,
            })
        })
        .collect();
    Ok(json!({
        "palette": palette,
        "count": colors.len(),
        "total": total,
        "truncated": take < total,
        "hint": if take < total {
            "只返回了前一部分 ⇒ 传 limit: 0 或更大的数可拿到全部"
        } else {
            "这是这个调色板的全部颜色"
        },
        "colors": colors,
    }))
}

fn write_brush_stroke(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let brush_name = require_str(args, "brush")?;
    let raw_points = args
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| missing("points"))?;
    if raw_points.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("points 不能为空 ⇒ 至少给一个 [x, y, pressure]"),
        ));
    }
    // **点的解析复用既有惯例** ✓（`[x,y]` 或 `[x,y,pressure]` 都收 ✓）。
    let mut points: Vec<(f64, f64, f64)> = Vec::new();
    for (index, raw) in raw_points.iter().enumerate() {
        let pair = raw.as_array().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("points[{index}] 不是数组 ⇒ 应为 [x, y, pressure]")),
            )
        })?;
        let x = pair.first().and_then(Value::as_f64).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("points[{index}][0] 不是数字")),
            )
        })?;
        let y = pair.get(1).and_then(Value::as_f64).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("points[{index}][1] 不是数字")),
            )
        })?;
        let pressure = pair
            .get(2)
            .and_then(Value::as_f64)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        points.push((x, y, pressure));
    }

    // **笔刷从缓存优先解析** ✓（用户导入的能覆盖内置的 ✓）。
    let name = if brush_name.ends_with(".myb") {
        brush_name.clone()
    } else {
        format!("{brush_name}.myb")
    };
    let path = ctx.workspace.resolve_asset("brush", &name)?;
    let json_text = std::fs::read_to_string(&path).map_err(|error| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("读不到笔刷 {}：{error}", path.display())),
        )
    })?;
    let mut brush: hokusai::Brush = hokusai::myb::from_str(&json_text).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{} 不是能解析的 .myb：{error}", path.display())),
        )
    })?;
    // **`size` 是"直径像素"** ✓，而 MyPaint 的设置叫 `radius_logarithmic` ✓（存的是 ln(半径) ✓）。
    if let Some(diameter) = args.get("size").and_then(Value::as_f64) {
        if diameter <= 0.0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("size 必须大于 0"),
            ));
        }
        // **`SettingValue` 是个结构体，不是枚举** ✗ —— 构造器是 `constant(f32)` ✓
        //（我第一版猜了 `Float(..)` ✓ ⇒ 编译器当场指出 ✓）。
        brush.set(
            hokusai::BrushSetting::Radius,
            hokusai::SettingValue::constant((diameter / 2.0).ln() as f32),
        );
    }

    let mut state = hokusai::BrushState::default();
    let mut surface = hokusai::tile_mem::MemSurface::new();
    let mut steps = 0usize;
    let mut previous: Option<(f64, f64, f64)> = None;
    for (x, y, pressure) in &points {
        match previous {
            None => {
                // **第一笔只播种位置** ✓（Hokusai 的语义 ✓）。
                brush.stroke_to(
                    &mut state,
                    &mut surface,
                    *x as f32,
                    *y as f32,
                    *pressure as f32,
                    0.0,
                    0.0,
                    0.01,
                );
                steps += 1;
            }
            Some((px, py, pp)) => {
                let distance = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                // **2px 步长** ✓：比任何笔尖都细 ✓ ⇒ 不会出现离散盖章 ✓（上限防病态输入 ✓）。
                let divisions = ((distance / 2.0).ceil() as usize).clamp(1, 4096);
                for step in 1..=divisions {
                    let t = step as f64 / divisions as f64;
                    let ix = px + (x - px) * t;
                    let iy = py + (y - py) * t;
                    let ip = pp + (pressure - pp) * t;
                    brush.stroke_to(
                        &mut state,
                        &mut surface,
                        ix as f32,
                        iy as f32,
                        ip as f32,
                        0.0,
                        0.0,
                        0.01,
                    );
                    steps += 1;
                }
            }
        }
        previous = Some((*x, *y, *pressure));
    }

    // **Hokusai 的 tile 是 fix15（u16, 0..32767）** ✓ ⇒ 转成我们用的 RGBA8 ✓（`>> 7` 正好 0..255 ✓）。
    // **Rust 不允许一条 `let` 里绑多个名字** ✗（我第一版写成 `let mut a = .., b = ..;` ✓
    // ⇒ 语法错 ✓）⇒ 拆开 ✓。
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for (x, y, _) in &points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    // **区域按控制点 + 笔刷半径外扩** ✓（笔尖会超出路径 ✓）；半径取 `size` 或保守的 64 ✓。
    let radius = args.get("size").and_then(Value::as_f64).unwrap_or(128.0) / 2.0 + 4.0;
    let x0 = (min_x - radius).floor() as i32;
    let y0 = (min_y - radius).floor() as i32;
    let x1 = (max_x + radius).ceil() as i32;
    let y1 = (max_y + radius).ceil() as i32;
    let width = (x1 - x0).max(1) as usize;
    let height = (y1 - y0).max(1) as usize;
    let mut rgba = vec![0u8; width * height * 4];
    let mut painted = 0usize;
    for tile_y in (y0.div_euclid(64))..=(y1.div_euclid(64)) {
        for tile_x in (x0.div_euclid(64))..=(x1.div_euclid(64)) {
            let Some(tile) = surface.tile(tile_x, tile_y) else {
                continue;
            };
            for row in 0..64i32 {
                for column in 0..64i32 {
                    // **fix15 → u8** ✓：`>> 7` 把 0..32767 映到 0..255 ✓。
                    let pixel = tile[row as usize][column as usize];
                    let r = (pixel[0] >> 7) as u8;
                    let g = (pixel[1] >> 7) as u8;
                    let b = (pixel[2] >> 7) as u8;
                    let a = (pixel[3] >> 7) as u8;
                    if a == 0 && r == 0 && g == 0 && b == 0 {
                        continue;
                    }
                    let doc_x = tile_x * 64 + column;
                    let doc_y = tile_y * 64 + row;
                    if doc_x < x0 || doc_y < y0 || doc_x >= x1 || doc_y >= y1 {
                        continue;
                    }
                    let at = ((doc_y - y0) as usize * width + (doc_x - x0) as usize) * 4;
                    rgba[at] = r;
                    rgba[at + 1] = g;
                    rgba[at + 2] = b;
                    rgba[at + 3] = a;
                    painted += 1;
                }
            }
        }
    }
    if painted == 0 {
        // **说清原因，并给出路** ✓（"错误里要能照着改" 是本项目的既定规矩 ✓）。
        //
        // **实测过的事** ✓：`ramon-Knife.myb` **无论给不给 `size` 都不出墨** ✗ ——
        // 它的 `dabs_per_basic_radius` 是 **0** ✓ ⇒ 按基本半径算间距时**间距趋于无穷** ✓
        // ⇒ **一枚印章都不落** ✗（`dabs_per_actual_radius` 那一套没被用上 ✓）。
        // ⇒ 这类笔刷**在当前引擎下画不出来** ✓ ⇒ 与其只报"没有墨" ✗，不如**指出来** ✓
        //（否则调用方会以为是自己参数给错 ✓ —— 我第一版的消息正是这样误导的 ✗）。
        let dabs_basic = brush
            .get(hokusai::BrushSetting::DabsPerBasicRadius)
            .base_value;
        let hint = if dabs_basic <= 0.0 {
            "这支笔刷的 `dabs_per_basic_radius` 是 0（间距按基本半径算 ⇒ 趋于无穷 ⇒ 一枚印章都不落）\
             ⇒ 它在当前引擎下画不出来，**换一支**；`assets/brushes` 里绝大多数都能画 ✓"
        } else {
            "试试给 size，或换一支笔刷 ✓"
        };
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("这一笔没落下任何像素 ⇒ 笔刷「{name}」{hint}")),
        ));
    }
    // **blob 先行** ✓，与 `medium_stroke` 完全同路 ✓。
    let composite = ctx.workspace.store().put(&rgba)?;
    let import_args = json!({
        "layer_id": layer_id,
        "object_id": optional_str(args, "object_id"),
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": x0, "y": y0, "w": width, "h": height},
    });
    let mut value = write_import_image(ctx, &import_args)?;
    value["brush"] = json!(name);
    value["steps"] = json!(steps);
    value["painted_pixels"] = json!(painted);
    value["region"] = json!({"x": x0, "y": y0, "w": width, "h": height});
    Ok(value)
}

fn write_list_assets(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let kind = require_str(args, "kind")?;
    let entries = ctx.workspace.list_assets(&kind)?;
    let items = asset_entries_to_json(&entries);
    // **"能不能导入"必须报出来** ✗（上一版把"没有工作区根目录"的错误**吞掉** ✓
    // ⇒ 纯内存模式返回空表 ✓ ⇒ 用户会以为"就是没有资产" ✗ ⇒ 那是在**骗人** ✓）。
    let can_import = ctx.workspace.asset_dir(&kind).is_ok();
    Ok(json!({
        "kind": kind,
        "count": items.len(),
        // **内置资产在任何模式下都能列出** ✓（它们在磁盘上 ✓）；
        // 但**缓存**需要工作区根目录 ✓ ⇒ 用 `can_import` 说清 ✓。
        "can_import": can_import,
        "hint": if !can_import {
            "纯内存模式 ⇒ 只能看到内置资产；要导入或抓取，请给服务器 --root（工作区目录）"
        } else if items.is_empty() {
            "这一类还没有资产 ⇒ 用 import_asset 导入，或跑 scripts/fetch-textures.sh 抓 CC0 纹理"
        } else {
            "source=bundled 随发行包发布；source=cache 是本地导入/抓取的（同名时后者生效）"
        },
        "assets": items,
    }))
}

/// **导入一件资产** ✓ —— 用户明确要求"**Web 与 MCP 都要能导入**" ✓。
///
/// **两个来源** ✓，因为两个调用方的能力不同 ✓：
/// * **`path`** ✓：服务器本地文件 ✓ —— MCP 与命令行最顺手 ✓；
/// * **`blob`** ✓：先走**已有的一次性上传**（`POST /api/blob` ✓）再把句柄给它 ✓ ——
///   **浏览器只能这么走** ✓（它拿不到服务器路径 ✓）。
///
/// **只做一条路会有一边用不了** ✗ ⇒ 两条都要 ✓，且**都汇进同一个内核方法** ✓（`import_asset` ✓）
/// ⇒ 校验、目录、覆盖语义**只有一份** ✓。
fn write_import_asset(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let kind = require_str(args, "kind")?;
    let name = require_str(args, "name")?;
    let overwrite = args
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let bytes = if let Some(path) = optional_str(args, "path") {
        std::fs::read(&path).map_err(|error| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("读不到 {path}：{error}")),
            )
        })?
    } else if let Some(blob) = args.get("blob") {
        let hash_text = blob
            .get("blob_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("blob 里缺少 blob_hash ⇒ 形如 {blob_hash,size,mime_type}"),
                )
            })?;
        let hash: yanshi_core::BlobHash = hash_text.parse().map_err(|_| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("blob_hash 不合法：{hash_text}")),
            )
        })?;
        ctx.workspace.store().get(&hash)?
    } else {
        return Err(missing("path 或 blob"));
    };
    let written = ctx
        .workspace
        .import_asset(&kind, &name, &bytes, overwrite)?;
    Ok(json!({
        "ok": true,
        "kind": kind,
        "name": name,
        "bytes": bytes.len(),
        "path": written.display().to_string(),
        "hint": "已进工作区缓存 ⇒ list_assets 立刻能看到；它不进 git ✓，也不会随发行包发布 ✓",
    }))
}

/// **资产列表转 JSON** ✓（三类共用 ✓ ⇒ 三处不会各变一样 ✗）。
/// **`usable` 直接取自内核** ✓ —— 工具层**不再自己判断** ✗（它曾经按扩展名猜 ✓，
/// 把一张灰度 PNG 报成可用 ✗ ⇒ 导入时被解码器拒绝 ✓）。
fn asset_entries_to_json(entries: &[crate::service::TextureEntry]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| {
            let extension = entry
                .name
                .rsplit_once('.')
                .map(|(_, ext)| ext.to_ascii_lowercase())
                .unwrap_or_default();
            json!({
                "name": entry.name,
                "bytes": entry.bytes,
                "source": entry.source,
                "format": extension,
                // **能不能直接用** ✓：**由内核判定** ✓（纹理要真读 PNG 头 ✗ ——
                // 我第一版在这里看扩展名猜 ✓ ⇒ 把一张灰度 PNG 报成可用 ✗，
                // 导入时被解码器拒绝 ✓ ⇒ 那是"说能用其实不能用" ✗）。
                "usable": entry.usable,
            })
        })
        .collect()
}

fn write_list_textures(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let dir = optional_str(args, "dir");
    let entries = ctx.workspace.list_textures(dir.as_deref())?;
    // **把"能不能直接用"一并报出来** ✓：内核**只解 PNG** ✗（JPEG/WebP 会被**明确拒绝** ✓）
    // ⇒ 与其让调用方试一次才知道 ✓，不如这里就说清 ✓ —— 与"错误里带可用选项"同一规矩 ✓。
    let items = asset_entries_to_json(&entries);
    Ok(json!({
        "dir": dir.unwrap_or_else(|| "textures".to_string()),
        "count": items.len(),
        // **空表是有意义的答案** ✓："还没下载过" ✓ ⇒ 提示去哪儿补 ✓（下一步写着脚本名 ✓）。
        "hint": if items.is_empty() {
            "没有可用纹理 ⇒ 跑 scripts/fetch-textures.sh --root <工作区> 抓取 CC0 纹理（默认来自 ambientCG ✓）"
        } else {
            "usable=true 的才能直接导入（内核只解 PNG ✓）；source=bundled 随发行包发布，source=cache 是本地抓的"
        },
        "textures": items,
    }))
}

fn write_export_project(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let path = require_str(args, "path")?;
    let doc_id = optional_str(args, "doc_id").unwrap_or_else(|| ctx.doc_id.clone());
    let (width, height) = ctx
        .workspace
        .document(&doc_id)
        .map(|document| (document.state().width, document.state().height))
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开 ⇒ 无法导出")),
            )
        })?;
    // **当场渲染一张整幅图** ✓（不是为了保存缓存 ✓，而是为了**包里那张预览必须是当前的** ✓）。
    let (_, _, pixels) = ctx.workspace.render_region_raw(
        &doc_id,
        yanshi_core::Bbox::new(0.0, 0.0, width as f64, height as f64),
    )?;
    let png = yanshi_render::png::encode_png(width, height, &pixels).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("PNG 编码失败（{width}×{height}）")),
        )
    })?;
    let tar = ctx.workspace.export_project(&doc_id, &png)?;
    std::fs::write(&path, &tar).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("写文件失败：{path} ⇒ {error}")),
        )
    })?;
    Ok(json!({
        "path": path,
        "doc_id": doc_id,
        "bytes": tar.len(),
        "render": {"width": width, "height": height, "bytes": png.len()},
        "format": "tar (uncompressed)",
    }))
}

/// **`export_png`** ✓ —— 把整幅（或指定区域）渲染成 PNG **落盘** ✓。
///
/// **为什么需要它** ✓（真实用户报的 §P2-6 ✓）：`render_region` 的 `include_image` **上限 512px** ✗
///（`preview.width <= 512 && preview.height <= 512` ✓），超了只回一个 `yanshi://blob/...` ✗
/// —— 那是**伪协议** ✓，进程外**取不到** ✗（用户原话 ✓）。
/// ⇒ agent 既不能抠 base64 ✓、也拿不到文件 ✓ —— 导出这一步**实际上没有出口** ✗。
///
/// **零件其实都齐** ✓，缺的只是把它们接起来 ✓：
/// `render_region_raw` ✓（取像素 ✓）＋ `yanshi_core::resample::resample_rgba` ✓（缩放 ✓）
/// ＋ `yanshi_render::png::encode_png` ✓（零依赖手写编码器 ✓）。
///
/// **语义** ✓：`mutating: false` ✓ —— 它**不改文档** ✓；但**它会写文件** ✓
/// ⇒ 这一点写进了 tools.md ✓，不假装它"只是读" ✗。
fn write_export_png(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let path = require_str(args, "path")?;
    // 缺省整幅 ✓：区域没给就用文档尺寸 ✓。
    let region = match args.get("region") {
        Some(value) if !value.is_null() => parse_bbox(value)?,
        _ => {
            let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
                )
            })?;
            let state = document.state();
            yanshi_core::Bbox::new(0.0, 0.0, state.width as f64, state.height as f64)
        }
    };
    let (width, height, pixels) = ctx.workspace.render_region_raw(&ctx.doc_id, region)?;
    if width == 0 || height == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("区域为空 ⇒ 没有可导出的像素"),
        ));
    }
    // **目标尺寸** ✓：显式宽高 ⇒ 用它；`max_edge` ⇒ 等比缩到最长边；都没给 ⇒ 原尺寸 ✓。
    let asked_width = optional_u64(args, "width").map(|value| value as u32);
    let asked_height = optional_u64(args, "height").map(|value| value as u32);
    let max_edge = optional_u64(args, "max_edge").map(|value| value as u32);
    if max_edge.is_some() && (asked_width.is_some() || asked_height.is_some()) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("max_edge 与 width/height 二选一 ✓，不要同时给"),
        ));
    }
    let (target_w, target_h) = match (asked_width, asked_height, max_edge) {
        (Some(w), Some(h), _) => (w, h),
        (None, None, Some(edge)) if edge > 0 => {
            let longest = width.max(height) as f64;
            let scale = (edge as f64 / longest).min(1.0);
            (
                ((width as f64 * scale).round() as u32).max(1),
                ((height as f64 * scale).round() as u32).max(1),
            )
        }
        (None, None, None) => (width, height),
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("width 与 height 要一起给 ✓（或改用 max_edge）"),
            ))
        }
    };
    let filter = match optional_str(args, "filter").as_deref() {
        None | Some("bilinear") => yanshi_core::resample::ResampleFilter::Bilinear,
        Some("nearest") => yanshi_core::resample::ResampleFilter::Nearest,
        Some(other) => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("未知缩放算法 {other} ✓（可用 nearest / bilinear）")),
            ))
        }
    };
    let scaled = if target_w == width && target_h == height {
        pixels
    } else {
        yanshi_core::resample::resample_rgba(&pixels, width, height, target_w, target_h, filter)
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "缩放失败 ✓（{width}×{height} ⇒ {target_w}×{target_h}）"
                    )),
                )
            })?
    };
    // `encode_png` 对**不可用的尺寸**返回 `None` ✓ ⇒ 按"参数不合法"报 ✓（`ErrorCode` 里没有 Internal ✓）。
    let png = yanshi_render::png::encode_png(target_w, target_h, &scaled).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "PNG 编码失败 ✓（{target_w}×{target_h} 不可用 ⇒ 请给一个合理尺寸）"
            )),
        )
    })?;
    // **整幅导出时顺手刷新渲染缓存** ✓（真实用户工程包里那张**空 `render.png`** 的根因 ✓）：
    // 缓存原本只在"整幅渲染 / 文档级缩略图"时更新 ✓，而导出走的是 `render_region_raw` ✓
    // ⇒ **绕过缓存** ✗ ⇒ 画完整幅画之后 ✓，磁盘上的预览**仍停在空白** ✗（他四个包实测 seq 5–6 vs 原子 97–217 ✓）。
    // **只在整幅时做** ✓：局部导出刷新缓存会把"打开即图片"变成一张**局部图** ✗。
    // **拿不到 head 也不阻断导出** ✓（缓存只是加速 ✓，不是产品 ✓）。
    let covers_frame = region.x <= 0.0
        && region.y <= 0.0
        && region.w
            >= ctx
                .workspace
                .document(&ctx.doc_id)
                .map(|document| document.state().width as f64)
                .unwrap_or(f64::MAX)
        && region.h
            >= ctx
                .workspace
                .document(&ctx.doc_id)
                .map(|document| document.state().height as f64)
                .unwrap_or(f64::MAX);
    // **真的落盘** ✓：这是这个工具存在的理由 ✓（不是再给一个拿不到的 URL ✗）。
    std::fs::write(&path, &png).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("写文件失败：{path} ⇒ {error}")),
        )
    })?;
    if covers_frame && target_w == width && target_h == height {
        // 只在**原尺寸整幅**时写缓存 ✓（缩放过的图不该当成"打开即图片" ✗）。
        let _ = ctx.workspace.cache_full_frame_png(&ctx.doc_id, &png);
    }
    Ok(json!({
        "path": path,
        "width": target_w,
        "height": target_h,
        "bytes": png.len(),
        "source_width": width,
        "source_height": height,
        "scaled": target_w != width || target_h != height,
    }))
}

/// **读取一条原子的完整记录（含净荷）** ✓ —— 补的是"**日志看得到、却不知道改了什么**"这个缺口 ✓。
///
/// **为什么需要它** ✓（本轮扫出来的 ✓）：全项目**只有 4 个只读工具会读净荷** ✓
///（`get_object_history` ✓、`list_comments` ✓、`list_suggestions` ✓、`preview_suggestion` ✓），
/// 而 `get_log` ✓ 与 `find_atom` ✓ **按设计只给元数据** ✗（前者是"MCP 轮询通道" ✓、后者是"检索" ✓）。
/// ⇒ 结果是：**界面上能列出"发生了什么" ✓，却问不出"这一条到底改了什么"** ✗ ——
/// 这正是我在评论面板上撞到过的那个缺口的**一般形式** ✓。
///
/// **为什么新加而不是改 `get_diff`** ✓：`get_diff`（设计 776 ✓）是"**两个序号之间的差分**" ✓，
/// 改它的响应会动到既有调用方 ✗；而"**按 id 取一条**"是另一个正交的需要 ✓（点开历史里某一条 ✓）。
/// **内核里本来就有** ✓：`Log::get(id)` ✓ ⇒ 这里只是把它**放行到工具面** ✓。
fn read_get_atom(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let atom_id = require_str(args, "atom_id")?;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let atom = document.log().get(&atom_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("原子 {atom_id} 不在日志里")),
        )
    })?;
    Ok(json!({
        "atom_id": atom.id.to_string(),
        "seq": atom.seq,
        "kind": kind_label(&atom.kind),
        "actor": atom.actor.to_string(),
        "session": atom.session.to_string(),
        "timestamp": atom.timestamp,
        "message": atom.message,
        "changeset_id": atom.changeset_id.as_ref().map(|id| id.to_string()),
        "parents": atom.parents.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        // **净荷就是这一条的全部意义所在** ✓（界面拿它显示"改了什么" ✓）。
        "payload": atom.payload,
    }))
}

/// **列出评论** ✓（协作通道的可读一侧 ✓）。
///
/// **为什么必须补这个工具** ✓（本轮实测出来的 ✓）：`comment` **能写** ✓，但**没有任何读工具** ✗ ——
/// `get_log` 与 `find_atom` 都只返回**元数据** ✓（设计如此 ✓：它们分别是"MCP 轮询通道"与"检索" ✓，
/// 返回 `actor/atom_id/kind/seq/timestamp` ✓，**不含净荷** ✗）⇒ 结果就是
/// **评论写进去、却读不回来** ✗。而同族的 `list_annotations` ✓ 与 `list_suggestions` ✓ **都是有的** ✓
/// ⇒ 评论缺了对称的那一半 ✓ —— 这是**产品缺口** ✓，不只是"界面少一块" ✓。
///
/// **净荷在日志里** ✓（`document.log()` 的原子带 `payload` ✓）⇒ 这里按通道过滤后把它取出来 ✓。
fn read_list_comments(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let since = optional_u64(args, "since_seq").unwrap_or(0);
    let limit = optional_u64(args, "limit").unwrap_or(50).clamp(1, 500) as usize;
    let actor = optional_str(args, "actor");
    let object_id = optional_str(args, "object_id");
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let mut comments = Vec::new();
    for atom in document.log().iter() {
        if atom.kind != AtomKind::Comment {
            continue;
        }
        let seq = atom.seq;
        if seq <= since {
            continue;
        }
        let actor_id = atom.actor.to_string();
        if let Some(wanted) = &actor {
            if &actor_id != wanted {
                continue;
            }
        }
        let payload = &atom.payload;
        let target_object = payload
            .get("object_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(wanted) = &object_id {
            if target_object.as_deref() != Some(wanted.as_str()) {
                continue;
            }
        }
        comments.push(json!({
            "atom_id": atom.id.to_string(),
            "seq": seq,
            "actor": actor_id,
            "session": atom.session.to_string(),
            "text": payload.get("text").and_then(Value::as_str).unwrap_or_default(),
            "target_atom": payload.get("target_atom").and_then(Value::as_str),
            "object_id": target_object,
            "timestamp": atom.timestamp,
        }));
    }
    let count = comments.len();
    // **最新的在前** ✓（评论是"读最近发生了什么" ✓），并保持 `since_seq` 的语义用**最大 seq** ✓。
    let next_since = comments
        .last()
        .and_then(|item| item.get("seq"))
        .and_then(Value::as_u64)
        .unwrap_or(since);
    comments.reverse();
    comments.truncate(limit);
    Ok(json!({
        "comments": comments,
        "count": count,
        "next_since": next_since,
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
    // **静默批处理** ✓（用户 §五-5 ✓）：`batch` 里的子调用**共用这个 `ctx`** ✓
    // ⇒ 在这里置一次 ✓ ⇒ **所有子调用都不再生成预览** ✓（一处生效、全部受益 ✓）。
    let previous_silent = ctx.silent;
    if optional_bool(args, "silent").unwrap_or(false) {
        ctx.silent = true;
    }
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
    // **还原静默标志** ✓：`ctx` 是**共享**的 ✓ ⇒ 不还原就会**漏到 batch 之后的调用** ✗
    //（那些调用本该照常带预览 ✓）。**共享上下文里"改了就要还原"** ✓ —— 与 `changeset` 同一规矩 ✓。
    ctx.silent = previous_silent;
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
        ObjectType::Path => "path",
    }
}
