//! 库元引擎 · view —— **界**（观测投影）：按视角过滤快照与事件流。
//! 契约：《引擎·标准》§10·5（界）· §3（可见性／归属字段）· §7（事件流）。
//!
//! 一句话：**投影只发生在观测侧**——世界照常演算（全部地址照常读写、事件照常记录），
//! 裁判看到的仍是全量；视角只决定"这份输出里有什么"。
//!
//! 三档（§10·5）：`真`＝人人可见 · `私`＝归属者＋裁判 · `裁判`＝仅裁判。
//! 视角＝**名字**：玩家视角＝其归属名；**保留名 `裁判`** ＝全见。

use kuyuan_core::{json_of_value, Vis, JUDGE};
use kuyuan_driver::World;
use kuyuan_log::Event;

/// 该视角是不是裁判（保留名，全见）。
pub fn is_judge(view: &str) -> bool {
    view == JUDGE
}

/// 某地址在某视角下是否可见：不在场＝不可见（对谁都一样）。
pub fn visible(w: &World, view: &str, addr: &str) -> bool {
    match w.index.get(addr) {
        None => false,
        Some(&i) => match w.vis[i] {
            Vis::Open => true,
            Vis::Owner => is_judge(view) || w.owner[i].as_deref() == Some(view),
            Vis::Judge => is_judge(view),
        },
    }
}

/// 该视角可见的地址（升序＝与全见快照同序）。
pub fn visible_addrs<'a>(w: &'a World, view: &str) -> Vec<&'a String> {
    w.addr_of.iter().filter(|a| visible(w, view, a)).collect()
}

/// 该视角可见的地址条数。**不报总数**——"共有多少"本身对观者是泄漏（S7 记）。
pub fn visible_count(w: &World, view: &str) -> usize {
    w.addr_of.iter().filter(|a| visible(w, view, a)).count()
}

/// 快照投影：只列可见地址（键升序，与全见快照同序）。
pub fn snapshot(w: &World, view: &str) -> serde_json::Value {
    let map: std::collections::BTreeMap<&String, serde_json::Value> = w
        .addr_of
        .iter()
        .enumerate()
        .filter(|(_, a)| visible(w, view, a))
        .map(|(i, a)| (a, json_of_value(&w.slots[i])))
        .collect();
    serde_json::to_value(map).unwrap_or(serde_json::Value::Null)
}

/// 快照投影的**文本形**：与 `World::snapshot_json` 同版式（pretty），故裁判视角逐字节相等。
pub fn snapshot_json(w: &World, view: &str) -> String {
    serde_json::to_string_pretty(&snapshot(w, view)).unwrap_or_else(|_| "{}".into())
}

/// 事件流投影（§10·5）：
/// - `目标` 不可见 → **整条不出现**（只藏值仍会泄漏"何时变过"）；
/// - `目标` **不在场**（包名／未解析） → 公开（加载期事实，非状态）；
/// - 投影内**重编号**（`序` 自 0 连续）——留空号会泄漏"隐藏地址变过几次、在第几拍"。
///
/// 原始日志**不动**（唯一事实源，D5）；这里出新的一份。
pub fn events(w: &World, events: &[Event], view: &str) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::new();
    for e in events {
        let keep = match w.index.get(&e.target) {
            None => true,
            Some(_) => visible(w, view, &e.target),
        };
        if keep {
            out.push(Event {
                seq: out.len() as u64,
                tick: e.tick,
                kind: e.kind.clone(),
                target: e.target.clone(),
                old: e.old.clone(),
                new: e.new.clone(),
                source: e.source.clone(),
            });
        }
    }
    out
}
