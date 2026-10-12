// 单机版（wasm 本地模式）无头冒烟 · 段①：单文件链路
// 用法：node engine/play/tests/solo_headless.js [wasm路径]
// 默认 wasm 路径＝库外构建产物（先跑：cargo build --release --target wasm32-unknown-unknown -p kuyuan-wasm）
// 覆盖：脚本加载 → 懒开局 → 动作结算 → 存档（localStorage）→ 终局 → 复盘。
'use strict';
const fs = require('fs');
const vm = require('vm');
const path = require('path');

const SRC = path.join(__dirname, '..', 'src');
const args = process.argv.slice(2);
const fileIdx = args.indexOf('--file');
let js, bridge, wasmB64 = null, packedB64 = null;
if (fileIdx >= 0){
  // --file 模式：直接读「打包后的单文件 HTML」（三段：页面 / base64 / 桥）
  const packedHtml = fs.readFileSync(args[fileIdx + 1], 'utf8');
  const scripts = [];
  const re = /<script>([\s\S]*?)<\/script>/g;
  let m;
  while ((m = re.exec(packedHtml))) scripts.push(m[1]);
  if (scripts.length < 3) throw new Error('单文件里 script 块 < 3（应＝页面/base64/桥）');
  js = scripts[0];
  packedB64 = scripts[1];
  bridge = scripts[2];
} else {
  // 源模式：读 src 三件 + 库外 wasm 产物
  const WASM = (args[0] && !args[0].startsWith('--'))
    ? args[0]
    : '/root/build/engine/wasm32-unknown-unknown/release/kuyuan_wasm.wasm';
  const page = fs.readFileSync(path.join(SRC, 'page.html'), 'utf8');
  bridge = fs.readFileSync(path.join(SRC, 'k32-local.js'), 'utf8');
  wasmB64 = fs.readFileSync(WASM).toString('base64');
  js = page.slice(page.indexOf('<script>') + 8, page.lastIndexOf('</script>'));
}

// ---------- 假 DOM ----------
const els = {};
function mkEl(id) {
  return {
    id, style: {}, textContent: '', innerHTML: '', value: '', min: '1', max: '6',
    disabled: false, scrollTop: 0, scrollHeight: 0, width: 640, height: 640, className: '',
    dataset: {}, classList: { toggle() {}, add() {}, remove() {} },
    getContext: () => new Proxy({}, { get: () => () => 0, set: () => true }),
    onclick: null, oninput: null
  };
}
function byId(id) { if (!els[id]) els[id] = mkEl(id); return els[id]; }
const actBtns = [0, 1, 2, 3].map(a => { const e = mkEl('a' + a); e.dataset = { act: String(a) }; return e; });
const dirBtns = [1, 2, 3, 4, 5, 6, 7, 8].map(d => { const e = mkEl('d' + d); e.dataset = { dir: String(d) }; return e; });
const doc = {
  getElementById: byId,
  querySelectorAll: sel => sel === '#acts button' ? actBtns : (sel === '#pad button[data-dir]' ? dirBtns : []),
  addEventListener: () => {}
};
const rafs = [];
const win = {
  addEventListener: () => {},
  requestAnimationFrame: cb => { rafs.push(cb); return rafs.length; },
  cancelAnimationFrame: () => {},
  performance: { now: () => Date.now() }
};
const memStore = {};
const storage = { getItem: k => (k in memStore ? memStore[k] : null), setItem: (k, v) => { memStore[k] = String(v); }, removeItem: k => { delete memStore[k]; } };

const sandbox = {
  window: win, document: doc, performance: win.performance,
  requestAnimationFrame: win.requestAnimationFrame, cancelAnimationFrame: win.cancelAnimationFrame,
  setTimeout, clearTimeout, console,
  localStorage: storage,
  WebAssembly, TextDecoder, TextEncoder, atob, BigInt,
  fetch: () => Promise.reject(new Error('本地模式不该走 fetch')),
  Date, Math, JSON, Object, Array, Number, String, Boolean, Error, Promise, Proxy, Reflect,
  parseInt, parseFloat, isNaN, isFinite, encodeURIComponent, decodeURIComponent
};
win.localStorage = storage;
sandbox.globalThis = sandbox;
vm.createContext(sandbox);

function run(code) { return vm.runInContext(code, sandbox, { filename: 'test' }); }
function assert(cond, msg) { if (!cond) throw new Error('ASSERT: ' + msg); }
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function until(fn, ms, what) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    let v = null;
    try { v = fn(); } catch (e) {}
    if (v) return v;
    await sleep(50);
  }
  throw new Error('等待超时：' + what);
}

(async () => {
  try {
    // ① 注入 base64 + 跑页面脚本 + 桥（＝单文件 HTML 的三段结构；--file 模式整包照跑）
    if (wasmB64 !== null){
      win.K32_WASM_BASE64 = wasmB64;
    } else {
      vm.runInContext(packedB64, sandbox, { filename: 'packed-b64.js' });
    }
    vm.runInContext(js, sandbox, { filename: 'page.js' });
    vm.runInContext(bridge, sandbox, { filename: 'k32-local.js' });
    console.log('① 页面 + wasm 桥加载 OK');

    // ② 手动 init（假 DOM 不触发 DOMContentLoaded）→ 懒开局
    run('init();');
    const st1 = await until(() => (run('S') && run('S').tick >= 1) ? run('JSON.parse(JSON.stringify(S))') : null, 8000, '首局');
    assert(st1.tick === 1 && st1.pos.length === 2, '首局形状');
    console.log('② 懒开局 OK · 对手 =', st1.opp_name, '· pos =', st1.pos.map(v => (+v).toFixed(1)));

    // ③ 打 6 拍
    for (let i = 0; i < 6; i++) {
      const t0 = run('S.tick');
      run('act = 1; dir = 4; submitAct();');
      await until(() => run('S.tick') > t0, 3000, '动作结算');
    }
    assert(run('S.tick') === 7, '6 拍后 tick=7');
    console.log('③ 6 拍结算 OK');

    // ④ 存档（localStorage 槽）
    run('doSave();');
    await until(() => run('$("msg").textContent.indexOf("已存档") >= 0'), 3000, '存档');
    assert(memStore['k32-save-slot1'], '存档应落槽');
    console.log('④ 存档 OK（字节 ' + memStore['k32-save-slot1'].length + '）');

    // ⑤ 打到终局
    for (let i = 0; i < 45 && run('S.result') === 0; i++) {
      const t0 = run('S.tick');
      run('act = 1; dir = 4; submitAct();');
      await until(() => run('S.tick') > t0 || run('S.result') !== 0, 3000, '推进');
    }
    assert(run('S.result') !== 0, '应到终局');
    console.log('⑤ 至终局 OK · tick =', run('S.tick'), '· result =', run('S.result'));

    // ⑥ 复盘
    run('loadReplay(' + run('S.tick') + ');');
    const R = await until(() => run('R && R.p1 && R.p2') ? run('JSON.parse(JSON.stringify(R))') : null, 4000, '复盘');
    assert(R.tick >= 2 && R.total >= R.tick, '复盘形状');
    console.log('⑥ 复盘 OK · 第', R.tick, '拍 · 共', R.total, '拍');

    console.log('ALL OK · 单机版（wasm 本地模式）无头全通');
  } catch (e) {
    console.error('FAILED:', e && e.stack || e);
    process.exit(1);
  }
})();