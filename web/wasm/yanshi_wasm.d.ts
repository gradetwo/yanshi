/* tslint:disable */
/* eslint-disable */

/**
 * 浏览器端的计算内核句柄。
 */
export class WasmKernel {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * 应用一个原子并本地乐观渲染；返回 13.1 风格的 dirty 报告。
     */
    apply_atom_json(atom_json: string): string;
    /**
     * 读取本地 blob。
     */
    blob_get(hash: string): Uint8Array;
    /**
     * 写入本地 blob（6.3 blob 先行），返回 CAS 哈希。
     */
    blob_put(bytes: Uint8Array): string;
    /**
     * 清除本地待提交覆盖层，返回需要重绘的区域。
     */
    clear_preview(): string;
    /**
     * 落笔提交：合并进本地日志并**失效重算受影响 tile**，返回
     * `{ok, report: {seq, dirty_bbox, dirty_tiles, head}}`。
     *
     * 调用方应据此重绘 `dirty_bbox`；不要再假定"覆盖层像素已在 tile 里"（那条假设在
     * 覆盖层与提交原子落在不同图层时不成立，会造成落笔后画布空白）。
     */
    commit_preview(atom_json: string): string;
    /**
     * 13.3：JS 侧按视口主动淘汰。
     */
    evict_outside_viewport(x: number, y: number, w: number, h: number): number;
    /**
     * **增量**更新待提交笔迹（拖动中的每一帧走这条路：成本 ∝ 新增笔段）。
     */
    extend_preview_stroke(json: string): string;
    /**
     * 是否存在待提交覆盖层。
     */
    has_preview(): boolean;
    /**
     * HEAD seq。
     */
    head_seq(): number;
    /**
     * 最近一次 dirty 的 tile（JSON 数组）。
     */
    last_dirty_tiles(): string;
    /**
     * view 模式批量装载：`json_array` 是服务端 `get_log` 给出的原子数组。
     */
    load_atoms_json(json_array: string): string;
    /**
     * 当前内存占用（字节）。
     */
    memory_usage(): number;
    /**
     * 水位比例（`used / limit`）。
     */
    memory_watermark(): number;
    /**
     * **构造一个内核实例** ✓（文档注释被我的插入"抢走"过一次 ✗ ⇒ 这是**第二次**踩同一个坑 ✓）。
     */
    constructor(doc_id: string, tile_size: number, width: number, height: number, memory_limit: number);
    /**
     * 新建内核：`doc_id`、tile 尺寸（32/64/128/256/512）、画布宽高、内存硬上限（字节）。
     * **笔刷预览** ✓（(A)③：把门面那件事搬进内核 ⇒ **一份实现** ✓）。
     * 收一段 JSON 请求 ⇒ 成功返回像素 ✓；失败返回 `undefined` ✓（**与"零长度成功"可区分** ✓）。
     */
    paint_brush(request_json: string): Uint8Array;
    /**
     * **上一次 `paint_brush` 失败的原因** ✓（成功时为空串 ✓，取走即清 ✓）。
     */
    paint_brush_error(): string;
    /**
     * 直绘一个小区域（拖动中的笔迹反馈）。
     *
     * 查看器一直依赖它；此前该方法**并不存在**，JS 抛 `TypeError: ... is not a function`
     * 被事件处理器吞掉，于是拖动与落笔后画布都没有内容（「操作后画布空白」缺陷）。
     */
    render_region_direct_rgba(x: number, y: number, w: number, h: number): Uint8Array;
    /**
     * 渲染区域并返回完整元信息（JSON：bbox/宽高/padding/警告/tile 数）。
     */
    render_region_info(x: number, y: number, w: number, h: number): string;
    /**
     * 渲染区域，返回 PNG 字节（与服务端同一编码器，可直接比对哈希）。
     */
    render_region_png(x: number, y: number, w: number, h: number): Uint8Array;
    /**
     * 渲染区域，返回直通 RGBA8（`Uint8Array`）。
     */
    render_region_rgba(x: number, y: number, w: number, h: number): Uint8Array;
    /**
     * 调整内存硬上限（字节）；突破水位时立即淘汰。
     */
    set_memory_limit(bytes: number): void;
    /**
     * 设置/更新**本地待提交覆盖层**（拖动中的笔迹），返回需要重绘的区域。
     *
     * 覆盖层不进原子日志：落笔时才用 `apply_atom_json` + `POST /api/atoms` 提交最终原子。
     */
    set_preview_object(json: string): string;
    /**
     * 设置视口（水位兜底会优先保留视口内 tile）。
     */
    set_viewport(x: number, y: number, w: number, h: number): void;
    /**
     * 状态摘要 JSON。
     */
    state_json(): string;
    /**
     * 统计 JSON（14.9 可观测性：命中率、淘汰、水位、自动淘汰次数）。
     */
    stats_json(): string;
    /**
     * 本地日志版本号（与服务端 5.7 错误里的 version 对照）。
     */
    version(): string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmkernel_free: (a: number, b: number) => void;
    readonly wasmkernel_apply_atom_json: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_blob_get: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_blob_put: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_clear_preview: (a: number) => [number, number];
    readonly wasmkernel_commit_preview: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_evict_outside_viewport: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly wasmkernel_extend_preview_stroke: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_has_preview: (a: number) => number;
    readonly wasmkernel_head_seq: (a: number) => number;
    readonly wasmkernel_last_dirty_tiles: (a: number) => [number, number];
    readonly wasmkernel_load_atoms_json: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_memory_usage: (a: number) => number;
    readonly wasmkernel_memory_watermark: (a: number) => number;
    readonly wasmkernel_new: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly wasmkernel_paint_brush: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_paint_brush_error: (a: number) => [number, number];
    readonly wasmkernel_render_region_direct_rgba: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmkernel_render_region_info: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmkernel_render_region_png: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmkernel_render_region_rgba: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmkernel_set_memory_limit: (a: number, b: number) => void;
    readonly wasmkernel_set_preview_object: (a: number, b: number, c: number) => [number, number];
    readonly wasmkernel_set_viewport: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmkernel_state_json: (a: number) => [number, number];
    readonly wasmkernel_stats_json: (a: number) => [number, number];
    readonly wasmkernel_version: (a: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
