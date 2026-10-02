// **viewer 角色在 WebSocket 上也不得改文档** ✓（真实漏洞的回归验收 ✓）。
//
// **漏洞原貌** ✓：角色检查原来只在 **HTTP 入口** ✓ ⇒ 堵住了 `/api/tools/*` ✓，
// 而 WebSocket 走另一条路 ✓、且**无条件** `with_owner(true)` ✗
// ⇒ **同一个 viewer 令牌，HTTP 被拒、WS 却成功** ✗。
// **修法** ✓：检查下沉到 `ToolRegistry::call` ✓（所有入口的必经之路 ✓）。
//
// **三条判据** ✓（都用**同一个 viewer 令牌** ✓）：
//   ① HTTP 调会改文档的工具 ⇒ 必须 permission_denied ✓；
//   ② **WebSocket 调同一个工具 ⇒ 也必须 permission_denied** ✓（这一条就是本脚本存在的理由 ✓）；
//   ③ **editor 令牌必须仍然能改** ✓ —— 别把漏洞修成"谁都不能改" ✗。
//
// 用法：`node scripts/acceptance/viewer-role-websocket.js <port>`
const port = Number(process.argv[2] || 8251);
const base = 'http://127.0.0.1:' + port;

async function createDocument(docId, role) {
  const response = await fetch(base + '/api/documents?role=' + role, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ doc_id: docId, width: 120, height: 90 }),
  });
  return response.json();
}

async function httpTool(docId, token, name, args) {
  const response = await fetch(base + '/api/tools/' + name + '?doc=' + docId + '&token=' + token, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(args),
  });
  return response.json();
}

function wsTool(docId, token, name, args) {
  return new Promise((resolve) => {
    const socket = new WebSocket('ws://127.0.0.1:' + port + '/ws?doc=' + docId + '&token=' + token);
    const finish = (value) => { try { socket.close(); } catch (_) {} resolve(value); };
    socket.addEventListener('open', () => {
      socket.send(JSON.stringify({ type: 'tool', request_id: 1, name: name, arguments: args }));
    });
    socket.addEventListener('message', (event) => {
      try {
        const parsed = JSON.parse(String(event.data));
        if (parsed.request_id === 1) finish(parsed.result || {});
      } catch (_) { /* 非 JSON 帧忽略 ✓ */ }
    });
    setTimeout(() => finish({ ok: null, error_code: 'timeout' }), 5000);
  });
}

(async () => {
  // **每次用唯一 id** ✓：文档 id 就是持久单元 ✓ ⇒ 重复用同一个 id 会被**拒绝** ✗
  //（`new_document` 的语义就是"新画布 = 新 id" ✓）⇒ 不唯一的话，**第二次跑这个脚本就会失败** ✗
  // —— 我第一版正是这样 ✗，而且失败信息（`ok:false` 无 error_code）一度让我以为是授权问题 ✗。
  const stamp = Date.now().toString(36);
  const viewer = await createDocument('sec_viewer_' + stamp, 'viewer');
  const editor = await createDocument('sec_editor_' + stamp, 'editor');
  const out = {};

  out.httpViewer = await httpTool(viewer.doc_id || ('sec_viewer_' + stamp), viewer.token, 'create_layer', { layer_id: 'L_http' });
  out.wsViewer = await wsTool(viewer.doc_id || ('sec_viewer_' + stamp), viewer.token, 'create_layer', { layer_id: 'L_ws' });
  out.httpEditor = await httpTool(editor.doc_id || ('sec_editor_' + stamp), editor.token, 'create_layer', { layer_id: 'L_editor' });

  const lines = [
    '① HTTP viewer    ⇒ ' + JSON.stringify({ ok: out.httpViewer.ok, code: out.httpViewer.error_code }),
    '② WS   viewer    ⇒ ' + JSON.stringify({ ok: out.wsViewer.ok, code: out.wsViewer.error_code }) + '   ← 本脚本的重点 ✓',
    '③ HTTP editor    ⇒ ' + JSON.stringify({ ok: out.httpEditor.ok, layer: out.httpEditor.layer_id }),
  ];
  const passed =
    out.httpViewer.ok === false &&
    out.wsViewer.ok === false &&
    out.httpEditor.ok === true;
  // **直接 node 跑时要自己打印** ✓（CDP 那些脚本是"返回给驱动" ✓，这条不是 ✓）。
  for (const line of lines) console.log('  ' + line);
  console.log('  ⇒ ' + (passed ? '通过 ✓' : '失败 ✗'));
  if (!passed) process.exitCode = 1;
  return passed;
})()
