//! S5′ · 边界断言 —— 《试装记录》§五 十条边界逐条转断言（真输入驱动）。
//!
//! 对应条例见 `packs/32×32/试装记录.md` §五；「40 拍平局」已在 `packs.rs::duel32_empty_run_is_draw`。

use kuyuan_verify as kv;

type Frames = Vec<(u64, &'static str, kv::Value)>;

const P1: [&str; 4] = [
    "duel32.玩家1.动作",
    "duel32.玩家1.方向dx",
    "duel32.玩家1.方向dy",
    "duel32.玩家1.射程",
];
const P2: [&str; 4] = [
    "duel32.玩家2.动作",
    "duel32.玩家2.方向dx",
    "duel32.玩家2.方向dy",
    "duel32.玩家2.射程",
];

fn addr(who: usize) -> [&'static str; 4] {
    if who == 1 { P1 } else { P2 }
}

/// 移动一拍。
fn mv(t: u64, who: usize, dx: f64, dy: f64, f: &mut Frames) {
    let a = addr(who);
    f.push((t, a[0], kv::Value::Int(1)));
    f.push((t, a[1], kv::Value::Real(dx)));
    f.push((t, a[2], kv::Value::Real(dy)));
    f.push((t, a[3], kv::Value::Real(0.0)));
}

/// 发射一拍（kind：2 探 · 3 爆）。
fn fire(t: u64, who: usize, kind: i64, dx: f64, dy: f64, range: f64, f: &mut Frames) {
    let a = addr(who);
    f.push((t, a[0], kv::Value::Int(kind)));
    f.push((t, a[1], kv::Value::Real(dx)));
    f.push((t, a[2], kv::Value::Real(dy)));
    f.push((t, a[3], kv::Value::Real(range)));
}

const D: f64 = 0.7071067811865476; // 1/√2
const GAP: f64 = 6.627416997969522; // 各走 8 拍后双方距离

/// ① 越界移动：一路撞墙 ⇒ 停在界内，不越界。
#[test]
fn boundary_move_out_of_bounds() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    for t in 1..=20u64 {
        mv(t, 1, -D, -D, &mut f);
    }
    let r = kv::run_pack_inputs(&text, 20, &f).expect("应跑通");
    let x = kv::get(&r.snapshot, "duel32.玩家1.x").as_f64().unwrap_or(-1.0);
    let y = kv::get(&r.snapshot, "duel32.玩家1.y").as_f64().unwrap_or(-1.0);
    assert!((0.0..=32.0).contains(&x) && (0.0..=32.0).contains(&y), "越界移动后仍在界内：({x}, {y})");
    assert!(x < 1.0 && y < 1.0, "应停在边界附近：( {x} , {y} )");
}

/// ② 超射程：爆器射程 17 > 上限 16 ⇒ 不发射。
#[test]
fn boundary_bomb_range_too_far() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    fire(1, 1, 3, D, D, 17.0, &mut f);
    let r = kv::run_pack_inputs(&text, 1, &f).expect("应跑通");
    assert_eq!(
        kv::get(&r.snapshot, "duel32.玩家1爆.启用"),
        &serde_json::json!(false),
        "超射程不应发射"
    );
}

/// ③ 射程 0：探测器原地点扫 ⇒ 当拍启用。
#[test]
fn boundary_probe_range_zero() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    fire(1, 1, 2, 0.0, 0.0, 0.0, &mut f);
    let r = kv::run_pack_inputs(&text, 1, &f).expect("应跑通");
    assert_eq!(
        kv::get(&r.snapshot, "duel32.玩家1探1.启用"),
        &serde_json::json!(true),
        "射程 0 应原地起飞"
    );
}

/// ④ 落点越界：预定落点出界 ⇒ 禁发射。
#[test]
fn boundary_target_out_of_bounds() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    fire(1, 1, 3, -D, -D, 16.0, &mut f); // (8,8) 朝外 16 格 → 落点为负
    let r = kv::run_pack_inputs(&text, 1, &f).expect("应跑通");
    assert_eq!(
        kv::get(&r.snapshot, "duel32.玩家1爆.启用"),
        &serde_json::json!(false),
        "落点越界不应发射"
    );
}

/// ⑤ 自炸：射程 0 ⇒ 落点＝自身 ⇒ 命中自己。
#[test]
fn boundary_self_blast() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    fire(1, 1, 3, 1.0, 0.0, 0.0, &mut f);
    let r = kv::run_pack_inputs(&text, 4, &f).expect("应跑通");
    assert_eq!(
        kv::get(&r.snapshot, "duel32.玩家1.存活"),
        &serde_json::json!(false),
        "自炸应算"
    );
    assert_eq!(kv::get(&r.snapshot, "duel32.场景.结果"), &serde_json::json!(2), "结果应为「玩家1负」");
}

/// ⑥ 双方互炸 ⇒ 平局（结果 3）。
#[test]
fn boundary_mutual_blast_draw() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    for t in 1..=8u64 {
        mv(t, 1, D, D, &mut f);
        mv(t, 2, -D, -D, &mut f);
    }
    fire(9, 1, 3, D, D, GAP, &mut f);
    fire(9, 2, 3, -D, -D, GAP, &mut f);
    let r = kv::run_pack_inputs(&text, 12, &f).expect("应跑通");
    assert_eq!(kv::get(&r.snapshot, "duel32.玩家1.存活"), &serde_json::json!(false));
    assert_eq!(kv::get(&r.snapshot, "duel32.玩家2.存活"), &serde_json::json!(false));
    assert_eq!(kv::get(&r.snapshot, "duel32.场景.结果"), &serde_json::json!(3), "同时被命中＝平局");
}

/// ⑦ 冷却边界：发射后下一拍不能再发射，再下一拍可以。
#[test]
fn boundary_cooldown() {
    let text = kv::read_pack("32×32/duel32.json");
    // A：t1 发射、t2 抢发 ⇒ 探2 不该启用（冷却 1 拍）
    let mut a: Frames = vec![];
    fire(1, 1, 2, 0.0, 0.0, 0.0, &mut a);
    fire(2, 1, 2, 0.0, 0.0, 0.0, &mut a);
    let ra = kv::run_pack_inputs(&text, 2, &a).expect("应跑通");
    assert_eq!(
        kv::get(&ra.snapshot, "duel32.玩家1探2.启用"),
        &serde_json::json!(false),
        "冷却期内不应发射"
    );
    // B：t1 发射、t3 发射 ⇒ 探2 启用（冷却已过）
    let mut b: Frames = vec![];
    fire(1, 1, 2, 0.0, 0.0, 0.0, &mut b);
    fire(3, 1, 2, 0.0, 0.0, 0.0, &mut b);
    let rb = kv::run_pack_inputs(&text, 3, &b).expect("应跑通");
    assert_eq!(
        kv::get(&rb.snapshot, "duel32.玩家1探2.启用"),
        &serde_json::json!(true),
        "冷却过后应可发射"
    );
}

/// ⑧ 相切算中：落点与对方中心距离恰为 3+1 ＝ 4 ⇒ 命中（≤）。
#[test]
fn boundary_tangent_hit() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    // 玩家2 走到 (16,8)：先纵向 16 拍，再横向 8 拍（整数坐标，无浮点误差）
    for t in 1..=16u64 {
        mv(t, 2, 0.0, -1.0, &mut f);
    }
    for t in 17..=24u64 {
        mv(t, 2, -1.0, 0.0, &mut f);
    }
    // 玩家1 在 (8,8) 朝右发 4 格 → 落点 (12,8)，距 (16,8) 恰 4
    fire(25, 1, 3, 1.0, 0.0, 4.0, &mut f);
    let r = kv::run_pack_inputs(&text, 27, &f).expect("应跑通");
    assert_eq!(
        kv::get(&r.snapshot, "duel32.玩家2.存活"),
        &serde_json::json!(false),
        "相切应算中"
    );
}

/// ⑨ 警告不合并：两个探测器各扫到玩家2 一次 ⇒ **被扫者**的两条通道各记一次。
#[test]
fn boundary_warnings_are_independent() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut f: Frames = vec![];
    for t in 1..=16u64 {
        mv(t, 2, 0.0, -1.0, &mut f);
    }
    for t in 17..=24u64 {
        mv(t, 2, -1.0, 0.0, &mut f);
    }
    // 探1：t25 发 6 格 → 扫描圆心 (14,8)，距玩家2 (16,8) ＝ 2 ≤ 3 ⇒ 扫到
    fire(25, 1, 2, 1.0, 0.0, 6.0, &mut f);
    // 探2：t27 再发 6 格（冷却已过）⇒ 同样扫到
    fire(27, 1, 2, 1.0, 0.0, 6.0, &mut f);
    let r = kv::run_pack_inputs(&text, 32, &f).expect("应跑通");
    // 规则 §3：警告发给**被探测者**（此处为玩家2），每探测器一条独立通道。
    for a in ["duel32.玩家2.警b1", "duel32.玩家2.警b2"] {
        let hit = r
            .events
            .iter()
            .any(|e| e.target == a && e.new == serde_json::json!(true));
        assert!(hit, "{a} 应有「被扫到」记录（两通道不合并）");
    }
}
