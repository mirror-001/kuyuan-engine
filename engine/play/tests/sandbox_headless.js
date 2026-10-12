// 沙盒批 · 无头模拟 · 段①：策略接管（node 运行）
// 用法：node engine/play/tests/sandbox_headless.js
// 覆盖：正常接管数拍 / 关脚本 / 脚本异常即停 / 非法返回即停 / 语法错误不启动 / 终局自动停。
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
    id, style: {}, textContent: '', innerHTML: '', value: '', min: '1', max: '6',
    disabled: false, scrollTop: 0, scrollHeight: 0, width: 640, height: 640, className: '',
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

// ---------- fetch stub：内存对局 ----------
let acts = [];
let gTick = 1;
let gResult = 0;
function mkState(t, result) {
  return {
    tick: t, next: t + 1, result: result === undefined ? gResult : result, alive: true, pos: [10, 10], cooldown: 0,
    probe_round: 0, probes: [{ i: 1, active: false, from: [0, 0], to: [0, 0], cur: [0, 0], range: 0, flown: 0, heard: null }],
    bombs: [{ i: 1, armed: false, x: 0, y: 0 }], warns: [], reports: [], fires: [], hears: [], blasts: [], blast: null,
    frames: t, log: []
  };
}
const sandbox = {
  window: win, document: doc, performance: win.performance,
  requestAnimationFrame: win.requestAnimationFrame, cancelAnimationFrame: win.cancelAnimationFrame,
  setTimeout, clearTimeout, console,
  localStorage: { getItem: () => null, setItem: () => {} },
  fetch: (p, opt) => {
    const path = String(p);
    if (path.indexOf('/api/act') >= 0) {
      acts.push(JSON.parse(opt.body));
      gTick += 1;
      return Promise.resolve({ ok: true, json: async () => mkState(gTick) });
    }
    if (path.indexOf('/api/state') >= 0) return Promise.resolve({ ok: true, json: async () => mkState(gTick) });
    return Promise.reject(new Error('offline-stub'));
  }
};
vm.createContext(sandbox);
vm.runInContext(js, sandbox, { filename: 'page.js' });
console.log('script loaded OK ·', js.length, 'chars');
function run(code) { return vm.runInContext(code, sandbox, { filename: 'test' }); }
function pumpAll(step) {
  let n = 0;
  while (rafs.length && n < 400) { const cb = rafs.shift(); clock += step; cb(clock); n++; }
  if (rafs.length) throw new Error('rAF 未收敛');
}
const sleep = ms => new Promise(r => setTimeout(r, ms));
function assert(cond, msg) { if (!cond) throw new Error('ASSERT: ' + msg); }
function setCode(code) { sandbox.__c = code; run('$("pol_code").value = __c;'); }
function toggle() { run('$("pol_tog").onclick();'); }
const st = () => byId('pol_st').textContent;

(async () => {
  run('polGap = 5; init();');
  await sleep(40);
  assert(run('S !== null'), 'init 后 S 应就位');

  // A 正常接管：总是移动 →
  run('S = ' + JSON.stringify(mkState(1)) + '; render(); polMem = {};');
  acts = []; gResult = 0;
  setCode('return {act:1, dir:4};');
  toggle();
  await sleep(150);
  assert(run('polOn') === true, 'A: 应运行中');
  assert(acts.length >= 3, 'A: 应自动提交 ≥3 拍，实得 ' + acts.length);
  for (const b of acts) assert(b.act === 1 && b.dir === 4, 'A: 动作应为移动→，实得 ' + JSON.stringify(b));
  assert(st().indexOf('运行') >= 0, 'A: 状态应含「运行中」，实得 ' + st());
  assert(els['go'].disabled === true, 'A: 运行中应锁手动提交');
  toggle();
  assert(run('polOn') === false, 'A: 关闭后应停止');
  console.log('A OK · 自动提交 ' + acts.length + ' 拍 · 开头 ' + JSON.stringify(acts[0]));

  // B 脚本异常即停
  setCode('throw new Error("boom");');
  toggle();
  await sleep(80);
  assert(run('polOn') === false, 'B: 出错应停');
  assert(st().indexOf('出错') >= 0, 'B: 状态应含「出错」，实得 ' + st());
  console.log('B OK · ' + st());

  // C 非法返回即停
  setCode('return 42;');
  toggle();
  await sleep(80);
  assert(run('polOn') === false, 'C: 非法返回应停');
  assert(st().indexOf('不合法') >= 0, 'C: 状态应含「不合法」，实得 ' + st());
  console.log('C OK · ' + st());

  // D 语法错误不启动
  setCode('return {');
  toggle();
  await sleep(30);
  assert(run('polOn') === false, 'D: 语法错误不应启动');
  assert(st().indexOf('语法错误') >= 0, 'D: 状态应含「语法错误」，实得 ' + st());
  console.log('D OK · ' + st());

  // E 终局自动停
  gResult = 0; gTick = 1;
  run('S = ' + JSON.stringify(mkState(1)) + '; render(); polMem = {};');
  setCode('return {act:0};');
  toggle();
  await sleep(60);
  assert(run('polOn') === true, 'E: 应先运行');
  gResult = 1;
  await sleep(80);
  assert(run('polOn') === false, 'E: 终局应自动停');
  assert(st().indexOf('结束') >= 0, 'E: 状态应含「结束」，实得 ' + st());
  console.log('E OK · ' + st());

  // F 推断：载入示范 → 运行 → 事件注入 → 热图有值 → 绘制出现
  run('S = ' + JSON.stringify(mkState(1)) + '; render();');
  run('$("inf_code").value = DEMO_INFER; $("inf_tog").onclick();');
  assert(run('infOn') === true, 'F: 推断应运行');
  const s5 = mkState(5);
  s5.reports = [{ tick: 5, x: 20, y: 20, dx: 0, dy: 0 }];
  s5.hears = [{ tick: 5, i: 1, dx: 1, dy: 0 }];
  s5.probes = [{ i: 1, active: true, from: [5, 5], to: [5, 5], cur: [5, 5], range: 6, flown: 6, heard: null }];
  run('S = ' + JSON.stringify(s5) + '; render();');
  assert(run('obs.length') >= 2, 'F: 事件流应 ≥2，实得 ' + run('obs.length'));
  const hsum = run('heat.reduce((a, b) => a + b, 0)');
  assert(hsum > 0, 'F: 热图应有值，实得 ' + hsum);
  const fr0 = calls.fillRect || 0;
  run('drawLive(staticLive(S));');
  assert((calls.fillRect || 0) > fr0, 'F: 绘制应出现 fillRect');
  assert(byId('inf_st').textContent.indexOf('运行') >= 0, 'F: 状态应含「运行中」，实得 ' + byId('inf_st').textContent);
  console.log('F OK · 事件 ' + run('obs.length') + ' · 热值和 ' + hsum.toFixed(2) + ' · fillRect +' + ((calls.fillRect || 0) - fr0));

  // I 回放热图：实况快照 → 回放叠画（批三补）
  assert(run('heatLog[5] !== undefined'), 'I: 实况渲染应留下第5拍快照');
  run('heatLog[5] = new Float64Array(1024).fill(0.5);');
  const fr1 = calls.fillRect || 0;
  run('drawReplayDisp({ tick: 5, total: 9, effects: [], p1: sideStatic({ x: 1, y: 1, alive: true, probes: [], bombs: [] }), p2: sideStatic({ x: 2, y: 2, alive: true, probes: [], bombs: [] }) });');
  assert((calls.fillRect || 0) > fr1, 'I: 回放热图应出现 fillRect');
  console.log('I OK · 回放热图 fillRect +' + ((calls.fillRect || 0) - fr1));

  // K 看图打 v1：喂一个"热峰在(5,0)"的假图 → 应返回 act:3（朝峰炸）
  (function(){
    const b = run('new Function("观测", "记忆", DEMO_POLICY)(' +
      'Object.assign({}, S, { tick: 10, pos: [10, 10], cooldown: 0, heat: (function(){ const h = new Float64Array(1024); h.fill(0.1); h[5] = 0.5; return h; })() }), {})');
    assert(b && b.act === 3, 'K: 热峰够热应炸，实得 ' + JSON.stringify(b));
    assert(b.dir >= 1 && b.dir <= 8 && b.range > 4.05 && b.range <= 16, 'K: 炸参数应合法（朝峰），实得 ' + JSON.stringify(b));
    console.log('K OK · 看图打返回 ' + JSON.stringify(b));
    // 方向映射自检：热峰在左上 → 应 ↖(5)（旧表 5/7 两项互换的虫）
    const b2 = run('new Function("观测", "记忆", DEMO_POLICY)(Object.assign({}, S, { tick: 10, pos: [10, 10], cooldown: 0, heat: (function(){ const h = new Float64Array(1024); h.fill(0.1); h[2*32+2] = 0.5; return h; })() }), {})');
    assert(b2 && b2.act === 3 && b2.dir === 5, 'K2: 左上热峰应 ↖(5)，实得 ' + JSON.stringify(b2));
    console.log('K2 OK · 左上热峰 → ↖(5)');
    // K3 自卫例外：布置期内有报告（硬情报）→ 仍应反击
    const b3 = run('new Function("观测", "记忆", DEMO_POLICY)(Object.assign({}, S, { tick: 5, pos: [10, 10], cooldown: 0, reports: [{ tick: 5, x: 20, y: 10 }], heat: null }), {})');
    assert(b3 && b3.act === 3, 'K3: 布置期报告应反击，实得 ' + JSON.stringify(b3));
    console.log('K3 OK · 布置期自卫反击 → ' + JSON.stringify(b3));
    // K4 静图试探：图平静（全 0.1）＋tick12＋冷却 0 → 应扔"问路炮"（朝巡逻向、射程压 16）
    const b4 = run('new Function("观测", "记忆", DEMO_POLICY)(Object.assign({}, S, { tick: 12, pos: [10, 10], cooldown: 0, heat: (function(){ const h = new Float64Array(1024); h.fill(0.1); return h; })() }), {})');
    assert(b4 && b4.act === 3 && b4.range > 15.5, 'K4: 静图应问路炮（range 压 16），实得 ' + JSON.stringify(b4));
    console.log('K4 OK · 静图问路炮 → ' + JSON.stringify(b4));
  })();

  // J 守恒：总账拉回 TOT（1024 × 0.1）
  const tot = run('heat.reduce((a, b) => a + b, 0)');
  assert(Math.abs(tot - 1024 * 0.1) < 1, 'J: 总账应守恒≈102.4，实得 ' + tot);
  console.log('J OK · 总账 Σ=' + tot.toFixed(1));

  // G 推断脚本异常即停
  sandbox.__i = 'throw new Error("x");';
  run('$("inf_code").value = __i; $("inf_tog").onclick();');
  run('$("inf_tog").onclick();');
  await sleep(30);
  assert(run('infOn') === false, 'G: 出错应停');
  assert(byId('inf_st').textContent.indexOf('出错') >= 0, 'G: 状态应含「出错」，实得 ' + byId('inf_st').textContent);
  console.log('G OK · ' + byId('inf_st').textContent);

  // H 推断语法错误不启动
  sandbox.__i = 'return {';
  run('$("inf_code").value = __i; $("inf_tog").onclick();');
  await sleep(20);
  assert(run('infOn') === false, 'H: 语法错误不应启动');
  assert(byId('inf_st').textContent.indexOf('语法错误') >= 0, 'H: 状态应含「语法错误」，实得 ' + byId('inf_st').textContent);
  console.log('H OK · ' + byId('inf_st').textContent);

  pumpAll(50);
  console.log('ALL OK');
})().catch(e => { console.error('FAILED:', e.message); process.exit(1); });