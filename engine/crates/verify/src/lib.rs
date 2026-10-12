//! 库元引擎 · verify —— 测试网（回归夹具）。
//!
//! 本件只服务于验证：**允许依赖全部件**，不进运行时依赖图（见 `engine/README.md`「依赖方向」）。
//! 用法：`cargo test -p kuyuan-verify`。

use std::path::PathBuf;

pub use kuyuan_core::Value;
pub use kuyuan_log::Event;

/// packs 目录：`engine/crates/verify` 上三级＝工作区父目录（`packs/` 与 `engine/` 同级）。
pub fn packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs")
}

/// 读一个内容包（相对 `packs/`）。
pub fn read_pack(rel: &str) -> String {
    let p = packs_dir().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读包失败 {}: {e}", p.display()))
}

/// 包文本 → 世界（过门禁；被拒则返 Err）。
pub fn load_pack(text: &str) -> Result<kuyuan_driver::World, String> {
    let pack: kuyuan_core::Pack =
        serde_json::from_str(text).map_err(|e| format!("解析包失败: {e}"))?;
    kuyuan_driver::load(&pack)
}

/// 包文本 → 世界（**记日志的形**，S4）：返回（结果, 事件流）。被拒时事件流含一条「拒绝」。
pub fn load_pack_logged(
    text: &str,
) -> (Result<kuyuan_driver::World, String>, Vec<Event>) {
    let mut log = kuyuan_log::EventLog::new();
    let r = match serde_json::from_str::<kuyuan_core::Pack>(text) {
        Ok(pack) => kuyuan_driver::load_logged(&pack, &mut log),
        Err(e) => {
            let m = format!("解析包失败: {e}");
            log.reject_load("（未解析）", &m);
            Err(m)
        }
    };
    (r, log.events)
}

/// 一次运行的结果：事件流 ＋ 终态快照。
pub struct Run {
    pub events: Vec<Event>,
    pub snapshot: serde_json::Value,
}

/// 跑 N 拍。
pub fn run_pack(text: &str, ticks: u64) -> Result<Run, String> {
    let mut world = load_pack(text)?;
    let mut log = kuyuan_log::EventLog::new();
    for t in 1..=ticks {
        world.tick(t, &mut log).map_err(|e| format!("tick {t} 失败: {e}"))?;
    }
    let snapshot =
        serde_json::from_str(&world.snapshot_json()).map_err(|e| format!("快照解析失败: {e}"))?;
    Ok(Run { events: log.events, snapshot })
}

/// 跑 N 拍，带输入：`(tick, 地址, 值)` 三元组——同一 tick 的多条即一帧（§11 输入集）。
pub fn run_pack_inputs(text: &str, ticks: u64, inputs: &[(u64, &str, Value)]) -> Result<Run, String> {
    let mut world = load_pack(text)?;
    let mut log = kuyuan_log::EventLog::new();
    for t in 1..=ticks {
        let mut frame: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
        for (ft, addr, v) in inputs {
            if *ft == t {
                frame.insert((*addr).to_string(), v.clone());
            }
        }
        world
            .tick_in(t, &frame, &mut log)
            .map_err(|e| format!("tick {t} 失败: {e}"))?;
    }
    let snapshot =
        serde_json::from_str(&world.snapshot_json()).map_err(|e| format!("快照解析失败: {e}"))?;
    Ok(Run { events: log.events, snapshot })
}

/// 事件流的文本投影（确定性比较用）。
pub fn events_json(run: &Run) -> String {
    run.events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 取快照里的地址之值。
pub fn get<'a>(snapshot: &'a serde_json::Value, addr: &str) -> &'a serde_json::Value {
    snapshot.get(addr).unwrap_or_else(|| panic!("快照无地址: {addr}"))
}
