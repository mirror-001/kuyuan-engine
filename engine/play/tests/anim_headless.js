// 动效批 · 无头模拟（库内测试 · node 运行）
// 用法：node engine/play/tests/anim_headless.js
// 干什么：假 DOM/canvas 加载 page.html 脚本，把「拍间补间＋特效」逐帧泵一遍——
//        live 9 场景（移动/发射/滑飞/听炸/扫描/投掷/爆环/玫线/跳过）＋ 复盘 3 场景（快照/前进/投掷玫线）。
//        任何运行时异常或 rAF 不收敛 → 退出码非 0。
'use strict';
const fs = require('fs');
const vm = require('vm');
const path = require('path');

const FP = path.join(__dirname, '..', 'src', 'page.html');
const html = fs.readFileSync(FP, 'utf8');
const js = html.slice(html.indexOf('<script>') + 8, html.lastIndexOf('</script>'));

// ---------- 假 DOM / canvas ----------
const calls = {};
const CTX = new Proxy({}, {
  get: (t, p) => {
    if (p === '__calls') return calls;
    return (...a) => { calls[p] = (calls[p] || 0) + 1; return 0; };
  },
  set: (t, p, v) => { t[p] = v; return true; }
});
const els = {};
function mkEl(id) {
  return {
    id, style: {}, textContent: '', innerHTML: '', value: '4', min: '1', max: '6',
    disabled: false, scrollTop: 0, scrollHeight: 0, width: 640, height: 640,
    dataset: {}, classList: { toggle() {}, add() {}, remove() {} },
    getContext: () => CTX, onclick: null, oninput: null
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
let clock = 1000;
const rafs = [];
const win = {
  addEventListener: () => {},
  requestAnimationFrame: cb => { rafs.push(cb); return rafs.length; },
  cancelAnimationFrame: () => {},
  performance: { now: () => clock }
};
const sandbox = {
  window: win, document: doc, performance: win.performance,
  requestAnimationFrame: win.requestAnimationFrame, cancelAnimationFrame: win.cancelAnimationFrame,
  setTimeout, clearTimeout, console,
  localStorage: { getItem: () => null, setItem: () => {} },
  __FR: {},
  fetch: () => Promise.reject(new Error('offline-stub'))
};
vm.createContext(sandbox);
vm.runInContext(js, sandbox, { filename: 'page.js' });
console.log('script loaded OK ·', js.length, 'chars');

function pumpAll(step) {
  let n = 0;
  while (rafs.length && n < 400) {
    const cb = rafs.shift();
    clock += step;
    cb(clock);
    n++;
  }
  if (rafs.length) throw new Error('rAF 未收敛');
}
function run(code) { vm.runInContext(code, sandbox, { filename: 'test' }); }

function st(t, x, y, probes, bombs, extra) {
  return Object.assign({
    tick: t, next: t + 1, result: 0, alive: true, pos: [x, y], cooldown: 0, probe_round: 0,
    probes: probes || [], bombs: bombs || [], warns: [], reports: [], fires: [], hears: [], blasts: [],
    blast: null, frames: t, log: []
  }, extra || {});
}
const noProbe = () => [{ i: 1, active: false, from: [0, 0], to: [0, 0], cur: [0, 0], range: 0, flown: 0, heard: null }];
const noBomb = () => [{ i: 1, armed: false, x: 0, y: 0 }];

// ---------- live 链路 ----------
run('S = ' + JSON.stringify(st(5, 10, 16, noProbe(), noBomb())) + '; displayReset(); render();');
pumpAll(50);
run('S = ' + JSON.stringify(st(6, 11, 16, noProbe(), noBomb())) + '; render();');
pumpAll(60);
run('S = ' + JSON.stringify(st(7, 11, 16, [{ i: 1, active: true, from: [11, 16], to: [17, 16], cur: [11, 16], range: 6, flown: 0, heard: null }], noBomb())) + '; render();');
pumpAll(60);
run('S = ' + JSON.stringify(st(8, 11, 16, [{ i: 1, active: true, from: [11, 16], to: [17, 16], cur: [13, 16], range: 6, flown: 2, heard: null }], noBomb())) + '; render();');
pumpAll(60);
run('S = ' + JSON.stringify(st(9, 11, 16, [{ i: 1, active: true, from: [11, 16], to: [17, 16], cur: [15, 16], range: 6, flown: 4, heard: [0.86, 0.51] }], noBomb())) + '; render();');
pumpAll(80);
run('S = ' + JSON.stringify(st(10, 11, 16, [{ i: 1, active: false, from: [11, 16], to: [17, 16], cur: [17, 16], range: 6, flown: 6, heard: null }], noBomb())) + '; render();');
pumpAll(80);
run('S = ' + JSON.stringify(st(11, 11, 16, noProbe(), [{ i: 1, armed: true, x: 20, y: 16 }])) + '; render();');
pumpAll(80);
run('S = ' + JSON.stringify(st(12, 11, 16, noProbe(), [{ i: 1, armed: false, x: 20, y: 16 }], { blast: [-0.9, 0.3], blasts: [{ tick: 12, dx: -0.9, dy: 0.3 }] })) + '; render();');
pumpAll(80);
run('setSpeed("跳过"); S = ' + JSON.stringify(st(13, 12, 16, noProbe(), noBomb())) + '; render();');
pumpAll(10);
run('setSpeed("标准");');
console.log('live 序列 OK · stroke=', calls.stroke, 'arc=', calls.arc, 'fill=', calls.fill);

// ---------- 复盘链路 ----------
const F2 = { tick: 2, total: 12, result: 1, p1: { x: 9, y: 9, alive: true, probes: [], bombs: [noBomb()[0]] }, p2: { x: 23, y: 23, alive: true, probes: [], bombs: [noBomb()[0]] } };
const F3 = { tick: 3, total: 12, result: 1, p1: { x: 10, y: 10, alive: true, probes: [{ i: 1, active: true, from: [10, 10], to: [16, 16], cur: [10, 10], heard: null }], bombs: [noBomb()[0]] }, p2: { x: 22, y: 22, alive: true, probes: [], bombs: [{ i: 1, armed: true, x: 18, y: 20 }] } };
const F4 = { tick: 4, total: 12, result: 1, p1: { x: 10, y: 10, alive: true, probes: [{ i: 1, active: true, from: [10, 10], to: [16, 16], cur: [12, 12], heard: null }], bombs: [noBomb()[0]] }, p2: { x: 22, y: 22, alive: true, probes: [], bombs: [noBomb()[0]], blast: [-0.7, 0.7] } };

(async () => {
  sandbox.fetch = (p) => Promise.resolve({ ok: true, json: async () => (String(p).indexOf('/api/replay') >= 0 ? sandbox.__FR : { error: 'stub' }) });
  run('S = ' + JSON.stringify(st(12, 10, 10, noProbe(), noBomb(), { result: 1 })) + '; displayReset();');
  sandbox.__FR = F2;
  run('render();');
  await new Promise(r => setTimeout(r, 10));
  pumpAll(60);
  console.log('复盘快照 OK');
  sandbox.__FR = F3;
  run('loadReplay(3);');
  await new Promise(r => setTimeout(r, 10));
  pumpAll(60);
  console.log('复盘动画(前进) OK');
  sandbox.__FR = F4;
  run('loadReplay(4);');
  await new Promise(r => setTimeout(r, 10));
  pumpAll(60);
  console.log('复盘动画(投掷/玫线) OK');
  if (calls.stroke < 80) throw new Error('绘制调用太少：' + calls.stroke);
  console.log('ALL OK · 总 stroke=', calls.stroke);
})().catch(e => { console.error('FAIL:', e && e.stack || e); process.exit(1); });