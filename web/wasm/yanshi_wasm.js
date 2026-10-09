/**
 * 浏览器端的计算内核句柄。
 */
export class WasmKernel {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmKernelFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmkernel_free(ptr, 0);
    }
    /**
     * 应用一个原子并本地乐观渲染；返回 13.1 风格的 dirty 报告。
     * @param {string} atom_json
     * @returns {string}
     */
    apply_atom_json(atom_json) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(atom_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmkernel_apply_atom_json(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * 读取本地 blob。
     * @param {string} hash
     * @returns {Uint8Array}
     */
    blob_get(hash) {
        const ptr0 = passStringToWasm0(hash, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmkernel_blob_get(this.__wbg_ptr, ptr0, len0);
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * 写入本地 blob（6.3 blob 先行），返回 CAS 哈希。
     * @param {Uint8Array} bytes
     * @returns {string}
     */
    blob_put(bytes) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmkernel_blob_put(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * 清除本地待提交覆盖层，返回需要重绘的区域。
     * @returns {string}
     */
    clear_preview() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmkernel_clear_preview(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * 落笔提交：合并进本地日志并**失效重算受影响 tile**，返回
     * `{ok, report: {seq, dirty_bbox, dirty_tiles, head}}`。
     *
     * 调用方应据此重绘 `dirty_bbox`；不要再假定"覆盖层像素已在 tile 里"（那条假设在
     * 覆盖层与提交原子落在不同图层时不成立，会造成落笔后画布空白）。
     * @param {string} atom_json
     * @returns {string}
     */
    commit_preview(atom_json) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(atom_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmkernel_commit_preview(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * 13.3：JS 侧按视口主动淘汰。
     * @param {number} x
     * @param {number} y
     * @param {number} w
     * @param {number} h
     * @returns {number}
     */
    evict_outside_viewport(x, y, w, h) {
        const ret = wasm.wasmkernel_evict_outside_viewport(this.__wbg_ptr, x, y, w, h);
        return ret >>> 0;
    }
    /**
     * **增量**更新待提交笔迹（拖动中的每一帧走这条路：成本 ∝ 新增笔段）。
     * @param {string} json
     * @returns {string}
     */
    extend_preview_stroke(json) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmkernel_extend_preview_stroke(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * 是否存在待提交覆盖层。
     * @returns {boolean}
     */
    has_preview() {
        const ret = wasm.wasmkernel_has_preview(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * HEAD seq。
     * @returns {number}
     */
    head_seq() {
        const ret = wasm.wasmkernel_head_seq(this.__wbg_ptr);
        return ret;
    }
    /**
     * 最近一次 dirty 的 tile（JSON 数组）。
     * @returns {string}
     */
    last_dirty_tiles() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmkernel_last_dirty_tiles(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * view 模式批量装载：`json_array` 是服务端 `get_log` 给出的原子数组。
     * @param {string} json_array
     * @returns {string}
     */
    load_atoms_json(json_array) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(json_array, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmkernel_load_atoms_json(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * 当前内存占用（字节）。
     * @returns {number}
     */
    memory_usage() {
        const ret = wasm.wasmkernel_memory_usage(this.__wbg_ptr);
        return ret;
    }
    /**
     * 水位比例（`used / limit`）。
     * @returns {number}
     */
    memory_watermark() {
        const ret = wasm.wasmkernel_memory_watermark(this.__wbg_ptr);
        return ret;
    }
    /**
     * **构造一个内核实例** ✓（文档注释被我的插入"抢走"过一次 ✗ ⇒ 这是**第二次**踩同一个坑 ✓）。
     * @param {string} doc_id
     * @param {number} tile_size
     * @param {number} width
     * @param {number} height
     * @param {number} memory_limit
     */
    constructor(doc_id, tile_size, width, height, memory_limit) {
        const ptr0 = passStringToWasm0(doc_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmkernel_new(ptr0, len0, tile_size, width, height, memory_limit);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WasmKernelFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * 新建内核：`doc_id`、tile 尺寸（32/64/128/256/512）、画布宽高、内存硬上限（字节）。
     * **笔刷预览** ✓（(A)③：把门面那件事搬进内核 ⇒ **一份实现** ✓）。
     * 收一段 JSON 请求 ⇒ 成功返回像素 ✓；失败返回 `undefined` ✓（**与"零长度成功"可区分** ✓）。
     * @param {string} request_json
     * @returns {Uint8Array}
     */
    paint_brush(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmkernel_paint_brush(this.__wbg_ptr, ptr0, len0);
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * **上一次 `paint_brush` 失败的原因** ✓（成功时为空串 ✓，取走即清 ✓）。
     * @returns {string}
     */
    paint_brush_error() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmkernel_paint_brush_error(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * 直绘一个小区域（拖动中的笔迹反馈）。
     *
     * 查看器一直依赖它；此前该方法**并不存在**，JS 抛 `TypeError: ... is not a function`
     * 被事件处理器吞掉，于是拖动与落笔后画布都没有内容（「操作后画布空白」缺陷）。
     * @param {number} x
     * @param {number} y
     * @param {number} w
     * @param {number} h
     * @returns {Uint8Array}
     */
    render_region_direct_rgba(x, y, w, h) {
        const ret = wasm.wasmkernel_render_region_direct_rgba(this.__wbg_ptr, x, y, w, h);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * 渲染区域并返回完整元信息（JSON：bbox/宽高/padding/警告/tile 数）。
     * @param {number} x
     * @param {number} y
     * @param {number} w
     * @param {number} h
     * @returns {string}
     */
    render_region_info(x, y, w, h) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmkernel_render_region_info(this.__wbg_ptr, x, y, w, h);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * 渲染区域，返回 PNG 字节（与服务端同一编码器，可直接比对哈希）。
     * @param {number} x
     * @param {number} y
     * @param {number} w
     * @param {number} h
     * @returns {Uint8Array}
     */
    render_region_png(x, y, w, h) {
        const ret = wasm.wasmkernel_render_region_png(this.__wbg_ptr, x, y, w, h);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * 渲染区域，返回直通 RGBA8（`Uint8Array`）。
     * @param {number} x
     * @param {number} y
     * @param {number} w
     * @param {number} h
     * @returns {Uint8Array}
     */
    render_region_rgba(x, y, w, h) {
        const ret = wasm.wasmkernel_render_region_rgba(this.__wbg_ptr, x, y, w, h);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * 调整内存硬上限（字节）；突破水位时立即淘汰。
     * @param {number} bytes
     */
    set_memory_limit(bytes) {
        wasm.wasmkernel_set_memory_limit(this.__wbg_ptr, bytes);
    }
    /**
     * 设置/更新**本地待提交覆盖层**（拖动中的笔迹），返回需要重绘的区域。
     *
     * 覆盖层不进原子日志：落笔时才用 `apply_atom_json` + `POST /api/atoms` 提交最终原子。
     * @param {string} json
     * @returns {string}
     */
    set_preview_object(json) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmkernel_set_preview_object(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * 设置视口（水位兜底会优先保留视口内 tile）。
     * @param {number} x
     * @param {number} y
     * @param {number} w
     * @param {number} h
     */
    set_viewport(x, y, w, h) {
        wasm.wasmkernel_set_viewport(this.__wbg_ptr, x, y, w, h);
    }
    /**
     * **可选平滑** ✓ —— 与服务端 `brush_stroke.smooth` **同一条实现** ✓
     * （`yanshi_render::brush::catmull_rom_smooth` ✓、同一个细分数 ✓）。
     *
     * 收 `[[x, y, pressure], ...]` ✓ ⇒ 成功回 `{"ok":true,"points":[…]}` ✓、
     * 失败回 `{"ok":false,…}`（5.7 形状 ✓ —— 与内核其余导出同一套信封 ✓）。
     * **为什么离线落笔要它** ✓：服务端是**先平滑 ⇒ 再算区域 ⇒ 再落笔** ✓
     * ⇒ 离线要复现同一笔，必须走**同一个**平滑 ✓（见 `crate::brush::smooth_points_json` 的说明 ✓）。
     * @param {string} points_json
     * @returns {string}
     */
    smooth_stroke_json(points_json) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(points_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmkernel_smooth_stroke_json(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * 状态摘要 JSON。
     * @returns {string}
     */
    state_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmkernel_state_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * 统计 JSON（14.9 可观测性：命中率、淘汰、水位、自动淘汰次数）。
     * @returns {string}
     */
    stats_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmkernel_stats_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * 本地日志版本号（与服务端 5.7 错误里的 version 对照）。
     * @returns {string}
     */
    version() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmkernel_version(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WasmKernel.prototype[Symbol.dispose] = WasmKernel.prototype.free;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_41e9ee4f547fc59a: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbindgen_generic_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./yanshi_wasm_bg.js": import0,
    };
}

const WasmKernelFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmkernel_free(ptr, 1));

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('yanshi_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
