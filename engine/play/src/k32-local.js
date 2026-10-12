/* k32-local.js —— 32×32 单机壳：wasm 本地桥（把 /api/* 换成内存引擎）
 *
 * 用法 A（开发 · 双文件）：同目录放 kuyuan_wasm.wasm，页面在 page.html 的脚本之后引入本文件。
 * 用法 B（单文件分发）：打包时把 wasm 的 base64 注成全局 K32_WASM_BASE64，再引入本文件。
 *
 * 接口对齐 HTTP 壳：state / act / new / save / load / replay（存档落 localStorage 单槽）。
 */
(function(){
  'use strict';
  const WASM_FILE = 'kuyuan_wasm.wasm';
  let ex = null;
  const mem = () => new Uint8Array(ex.memory.buffer);

  const ready = (async () => {
    let bytes;
    const b64 = (typeof K32_WASM_BASE64 === 'string' && K32_WASM_BASE64.length)
      ? K32_WASM_BASE64
      : ((typeof window !== 'undefined' && window && typeof window.K32_WASM_BASE64 === 'string') ? window.K32_WASM_BASE64 : '');
    if (b64.length){
      const bin = atob(b64);
      const u = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i);
      bytes = u;
    } else {
      bytes = new Uint8Array(await (await fetch(WASM_FILE)).arrayBuffer());
    }
    const { instance } = await WebAssembly.instantiate(bytes, {});
    ex = instance.exports;
  })();

  function unpack(x){ const b = BigInt(x); return { ptr: Number(b >> 32n), len: Number(b & 0xFFFFFFFFn) }; }
  function readOut(packed){
    const { ptr, len } = unpack(packed);
    const s = new TextDecoder('utf-8').decode(mem().subarray(ptr, ptr + len));
    ex.k32_free(packed);
    return s;
  }
  function writeStr(s){
    const b = new TextEncoder().encode(s);
    const p = ex.k32_alloc(b.length);
    mem().set(b, p);
    return { p, n: b.length };
  }
  function callJson(fn, ...args){ return JSON.parse(readOut(ex[fn](...args))); }
  function mustJson(fn, ...args){ const r = callJson(fn, ...args); if (r && r.error) throw new Error(r.error); return r; }
  function newSeed(){
    const s = (BigInt(Date.now()) << 20n) | BigInt(Math.floor(Math.random() * 1048576));
    return [Number(s & 0xFFFFFFFFn), Number((s >> 32n) & 0xFFFFFFFFn)];
  }

  const store = (function(){
    try {
      if (typeof localStorage !== 'undefined' && localStorage){
        localStorage.setItem('__k32t', '1');
        localStorage.removeItem('__k32t');
        return localStorage;
      }
    } catch (e) {}
    const m = {};
    return { getItem: k => (k in m ? m[k] : null), setItem: (k, v) => { m[k] = String(v); }, removeItem: k => { delete m[k]; } };
  })();

  async function api(path, opt){
    await ready;
    const body = opt && opt.body ? String(opt.body) : '';
    if (path === '/api/state'){
      try { return mustJson('k32_state'); }
      catch (e){ const [lo, hi] = newSeed(); return mustJson('k32_new', lo, hi, 0, 0); } // 懒开局
    }
    if (path === '/api/act'){ const w = writeStr(body); return mustJson('k32_act', w.p, w.n); }
    if (path === '/api/new'){
      const [lo, hi] = newSeed();
      const w = writeStr(body || '');
      return mustJson('k32_new', lo, hi, w.p, w.n);
    }
    if (path === '/api/save'){
      const out = mustJson('k32_save');
      store.setItem('k32-save-slot1', JSON.stringify(out));
      return { ok: true, tick: out.tick };
    }
    if (path === '/api/load'){
      const raw = store.getItem('k32-save-slot1');
      if (!raw) throw new Error('没找到存档（本机槽）');
      const w = writeStr(raw);
      return mustJson('k32_load', w.p, w.n);
    }
    throw new Error('本地模式无此路：' + path);
  }

  function replay(n){
    return Promise.resolve().then(async () => {
      await ready;
      const r = callJson('k32_replay', (n >>> 0), 0);
      if (r && r.error) throw new Error(r.error);
      return r;
    });
  }

  if (typeof window !== 'undefined') window.K32_LOCAL = { api, replay, ready };
  if (typeof globalThis !== 'undefined') globalThis.K32_LOCAL = { api, replay, ready };
})();