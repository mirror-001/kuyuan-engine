//! 库元引擎 · wasm —— 32×32 单机壳的引擎胶水（浏览器 / 任意平台 · 单 HTML 分发）。
//!
//! 一句话：把 play 的会话核编译成 wasm——JS 侧以纯函数驱动一局对局。
//!
//! 内存协议（手写 C ABI，无 wasm-bindgen）：
//! - 输出：`u64`（高 32 位＝指针 · 低 32 位＝字节长）；JS 读内存取 UTF-8，用后调 `k32_free`
//! - 输入：JS 先调 `k32_alloc` 拿指针写入，再随指针/长度传入
//! - 语义对齐 HTTP 壳：state / act / new / save / load / replay（save、load 由 JS 落 localStorage）
//!
//! 单线程前提（浏览器主线程 / node）：会话存在 thread-local 里。

use std::cell::RefCell;

use kuyuan_play::session::{Opponent, Session};

/// 内嵌包：编译期嵌入——单文件分发不需要读盘。
const PACK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../packs/32×32/duel32.json"
));

thread_local! {
    static GAME: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// 输出打包：Ok→JSON 串；Err→`{"error": ...}`。
fn ret(r: Result<String, String>) -> u64 {
    let s = match r {
        Ok(s) => s,
        Err(e) => format!(
            "{{\"error\":{}}}",
            serde_json::to_string(&e).unwrap_or_else(|_| "\"?\"".to_string())
        ),
    };
    let boxed = s.into_bytes().into_boxed_slice(); // cap==len，JS 侧按同一规则释放
    let n = boxed.len() as u64;
    let p = boxed.as_ptr() as u64;
    std::mem::forget(boxed); // 由 JS 调 k32_free 归还
    (p << 32) | n
}

/// 读取并释放输入缓冲（JS 侧 k32_alloc 分配）。
unsafe fn take_in(p: *const u8, n: u32) -> String {
    let s = if p.is_null() || n == 0 {
        String::new()
    } else {
        unsafe { String::from_utf8_lossy(std::slice::from_raw_parts(p, n as usize)).to_string() }
    };
    if !p.is_null() && n > 0 {
        unsafe { drop(Vec::from_raw_parts(p as *mut u8, 0, n as usize)); }
    }
    s
}

/// 分配（JS 写输入用）。
#[unsafe(no_mangle)]
pub extern "C" fn k32_alloc(n: u32) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n as usize);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// 归还原输出内存（ptr＝打包的 u64）。
#[unsafe(no_mangle)]
pub extern "C" fn k32_free(packed: u64) {
    let p = (packed >> 32) as *mut u8;
    let n = (packed & 0xFFFF_FFFF) as usize;
    if p.is_null() || n == 0 {
        return;
    }
    unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, n)));
    }
}

/// 开新局。`opp_json` 为空＝默认「听风」；否则 `{"mode":"standard|hunter|random|dummy|custom", ...}`。
/// 返回新 state JSON。
#[unsafe(no_mangle)]
pub extern "C" fn k32_new(seed_lo: u32, seed_hi: u32, opp_ptr: *const u8, opp_len: u32) -> u64 {
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    let opp = unsafe { take_in(opp_ptr, opp_len) };
    let r = (|| -> Result<String, String> {
        let spec = if opp.trim().is_empty() {
            Opponent::default()
        } else {
            let v: serde_json::Value =
                serde_json::from_str(&opp).map_err(|e| format!("对手配置不是 JSON: {e}"))?;
            Opponent::request(&v)?
        };
        let s = Session::with_opponent(PACK, seed, spec)?;
        let out = s.state_json().to_string();
        GAME.with(|g| *g.borrow_mut() = Some(s));
        Ok(out)
    })();
    ret(r)
}

/// 当前状态（对齐 GET /api/state）。
#[unsafe(no_mangle)]
pub extern "C" fn k32_state() -> u64 {
    let r = GAME.with(|g| match &*g.borrow() {
        Some(s) => Ok(s.state_json().to_string()),
        None => Err("还没开局".to_string()),
    });
    ret(r)
}

/// 提交我方动作 `{"act":1,"dir":4,"range":6}`（对齐 POST /api/act）。返回新 state JSON。
#[unsafe(no_mangle)]
pub extern "C" fn k32_act(p: *const u8, n: u32) -> u64 {
    let body = unsafe { take_in(p, n) };
    let r = (|| -> Result<String, String> {
        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("请求体不是 JSON: {e}"))?;
        let act = v.get("act").and_then(|x| x.as_i64()).ok_or("缺 act")?;
        let dir = v.get("dir").and_then(|x| x.as_i64());
        let range = v.get("range").and_then(|x| x.as_f64());
        GAME.with(|g| {
            let mut g = g.borrow_mut();
            let s = g.as_mut().ok_or("还没开局")?;
            s.act(act, dir, range)?;
            Ok(s.state_json().to_string())
        })
    })();
    ret(r)
}

/// 复盘（对齐 GET /api/replay?t=N）。`upto`＝帧号（u64::MAX 亦可）。
#[unsafe(no_mangle)]
pub extern "C" fn k32_replay(upto_lo: u32, upto_hi: u32) -> u64 {
    let upto = ((upto_hi as u64) << 32) | upto_lo as u64;
    let r = GAME.with(|g| match &*g.borrow() {
        Some(s) => s.replay_json(upto).map(|v| v.to_string()),
        None => Err("还没开局".to_string()),
    });
    ret(r)
}

/// 存档：返回存档 JSON（JS 侧写 localStorage）。
#[unsafe(no_mangle)]
pub extern "C" fn k32_save() -> u64 {
    let r = GAME.with(|g| match &*g.borrow() {
        Some(s) => s.save_json().map(|v| {
            serde_json::json!({ "save": v, "tick": s.tick() }).to_string()
        }),
        None => Err("还没开局".to_string()),
    });
    ret(r)
}

/// 读档：传入存档 JSON（JS 侧从 localStorage 取出）＝`{"save": ...}` 或直接存档本体。
/// 返回新 state JSON。
#[unsafe(no_mangle)]
pub extern "C" fn k32_load(p: *const u8, n: u32) -> u64 {
    let body = unsafe { take_in(p, n) };
    let r = (|| -> Result<String, String> {
        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("存档不是 JSON: {e}"))?;
        let save = v.get("save").cloned().unwrap_or(v);
        let s = Session::load(PACK, &save)?;
        let out = s.state_json().to_string();
        GAME.with(|g| *g.borrow_mut() = Some(s));
        Ok(out)
    })();
    ret(r)
}
