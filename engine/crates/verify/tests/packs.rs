//! 内容包回归（S1）：三个包的可观测行为。
//!
//! 断言来源：`packs/00-说明.md`（L0／L1 断言草案）· `packs/32×32/duel32-说明.md`（L3）。

use kuyuan_verify as kv;

const DUEL: &str = "32×32/duel32.json";

/// L0：t1 计数 0→1；两拍后快照 计数＝2。
#[test]
fn l0_counter_two_ticks() {
    let r = kv::run_pack(&kv::read_pack("demo.计数器.json"), 2).expect("L0 应跑通");
    assert_eq!(r.events.len(), 2, "两拍两条事件");
    let e = &r.events[0];
    assert_eq!(e.tick, 1);
    assert_eq!(e.target, "demo.计数器.计数");
    assert_eq!(e.old, serde_json::json!(0));
    assert_eq!(e.new, serde_json::json!(1));
    assert_eq!(kv::get(&r.snapshot, "demo.计数器.计数"), &serde_json::json!(2));
}

/// L1：倍值滞后一拍——正是「读旧写新」的痕迹。
#[test]
fn l1_wire_reads_old_writes_new() {
    let r = kv::run_pack(&kv::read_pack("demo.接线.json"), 2).expect("L1 应跑通");
    assert_eq!(r.events.len(), 4, "两拍四条事件");
    assert!(
        !r.events.iter().any(|e| e.tick == 1 && e.target.ends_with("倍值")),
        "t1 不应出现倍值变化（入值 t1 仍为 0）"
    );
    let e = r
        .events
        .iter()
        .find(|e| e.target == "demo.加倍器.倍值")
        .expect("应有倍值变更事件");
    assert_eq!(e.tick, 2, "倍值在 t2 才变");
    assert_eq!(e.old, serde_json::json!(0));
    assert_eq!(e.new, serde_json::json!(2));
    assert_eq!(kv::get(&r.snapshot, "demo.加倍器.倍值"), &serde_json::json!(2));
    assert_eq!(kv::get(&r.snapshot, "demo.计数器.计数"), &serde_json::json!(2));
}

/// L3：空跑 42 拍＝49 条事件、回合 40、结果 3（平局）。
#[test]
fn duel32_empty_run_is_draw() {
    let r = kv::run_pack(&kv::read_pack(DUEL), 42).expect("duel32 应跑通");
    assert_eq!(r.events.len(), 49, "42 拍 49 条事件");
    assert_eq!(kv::get(&r.snapshot, "duel32.场景.回合"), &serde_json::json!(40));
    assert_eq!(kv::get(&r.snapshot, "duel32.场景.结果"), &serde_json::json!(3));
}

/// L3：尾部 8 条＝结束之后「结果 → 各消费者.局面」的接线搬运（不是游戏状态变更）。
#[test]
fn duel32_tail_eight_are_gate_carry() {
    let r = kv::run_pack(&kv::read_pack(DUEL), 42).expect("duel32 应跑通");
    let tail = &r.events[41..];
    assert_eq!(tail.len(), 8, "尾部应为 8 条搬运");
    assert!(
        tail.iter().all(|e| e.tick == 42 && e.target.ends_with("局面")),
        "尾部应全为 t42 的「局面」搬运，实得：{:?}",
        tail.iter().map(|e| (&e.target, e.tick)).collect::<Vec<_>>()
    );
}

/// L3：结束后冻结——43 拍不再产生新事件。
#[test]
fn duel32_freezes_after_end() {
    let a = kv::run_pack(&kv::read_pack(DUEL), 42).expect("42 拍应跑通");
    let b = kv::run_pack(&kv::read_pack(DUEL), 43).expect("43 拍应跑通");
    assert_eq!(a.events.len(), b.events.len(), "结束后不应再产生事件");
    assert_eq!(a.snapshot, b.snapshot, "结束后快照应不变");
}

/// D6 确定性：同输入 → 同输出（事件流与快照逐字节相同）。
#[test]
fn duel32_deterministic() {
    let a = kv::run_pack(&kv::read_pack(DUEL), 42).expect("duel32 应跑通");
    let b = kv::run_pack(&kv::read_pack(DUEL), 42).expect("duel32 应跑通");
    assert_eq!(kv::events_json(&a), kv::events_json(&b), "事件流应逐字节相同");
    assert_eq!(a.snapshot, b.snapshot, "快照应相同");
}

/// S2a：运行期能分辨「输入」与「变量」——分区化的立足点。
#[test]
fn world_knows_inputs_and_vars() {
    let w = kv::load_pack(&kv::read_pack(DUEL)).expect("duel32 应加载");
    assert!(w.is_input("duel32.玩家1.动作"), "动作应是输入");
    assert!(w.is_var("duel32.场景.结果"), "结果应是变量");
    assert!(!w.is_input("duel32.场景.结果"), "变量不算输入");
    assert!(!w.is_var("duel32.玩家1.动作"), "输入不算变量");
    assert_eq!(w.inputs().count(), 144, "duel32 输入 144 个地址");
    assert_eq!(w.vars().count(), 102, "duel32 变量 102 个地址（v0.3 · 听炸 +12）");
}

// ---------- S5′：输入集（《标准》§11） ----------

/// §11·5：注入是本拍初值的一部分——当拍即被公式读到，并经接线传到下游。
#[test]
fn input_reaches_formula_in_same_tick() {
    let text = kv::read_pack(DUEL);
    let r = kv::run_pack_inputs(
        &text,
        1,
        &[
            (1, "duel32.玩家1.动作", kv::Value::Int(2)),
            (1, "duel32.玩家1.射程", kv::Value::Real(0.0)),
            (1, "duel32.玩家1.方向dx", kv::Value::Real(0.0)),
            (1, "duel32.玩家1.方向dy", kv::Value::Real(0.0)),
        ],
    )
    .expect("注入应跑通");
    assert_eq!(
        kv::get(&r.snapshot, "duel32.玩家1探1.启用"),
        &serde_json::json!(true),
        "当拍发射应生效（注入经接线到达探池）"
    );
    assert!(r.events.iter().any(|e| e.kind == "输入注入"), "应有「输入注入」事件");
}

/// §11·1：只许写输入——写变量应拒。
#[test]
fn input_rejects_variable_write() {
    let text = kv::read_pack(DUEL);
    let err = kv::run_pack_inputs(&text, 1, &[(1, "duel32.场景.结果", kv::Value::Int(1))])
        .err()
        .expect("写变量应被拒");
    assert!(err.contains("不是输入"), "实得：{err}");
}

/// §11·2：被接线的输入＝引擎内部信号，拒绝外部注入。
#[test]
fn input_rejects_wired_input() {
    let text = kv::read_pack(DUEL);
    let err = kv::run_pack_inputs(&text, 1, &[(1, "duel32.玩家1.局面", kv::Value::Int(0))])
        .err()
        .expect("写被接线的输入应被拒");
    assert!(err.contains("已被接线"), "实得：{err}");
}

/// §11·1：写不存在的地址应拒。
#[test]
fn input_rejects_unknown_address() {
    let text = kv::read_pack(DUEL);
    let err = kv::run_pack_inputs(&text, 1, &[(1, "duel32.不存在", kv::Value::Int(0))])
        .err()
        .expect("写不存在地址应被拒");
    assert!(err.contains("不在场"), "实得：{err}");
}

/// S5′ 首场真对局：双方各走 8 拍靠近，P1 第 9 拍发射爆炸器命中 → P1 胜。
#[test]
fn duel32_real_duel_player1_wins() {
    let text = kv::read_pack(DUEL);
    let d = 0.7071067811865476_f64; // 1/√2
    let mut frames: Vec<(u64, &str, kv::Value)> = vec![];
    for t in 1..=8u64 {
        frames.push((t, "duel32.玩家1.动作", kv::Value::Int(1)));
        frames.push((t, "duel32.玩家1.方向dx", kv::Value::Real(d)));
        frames.push((t, "duel32.玩家1.方向dy", kv::Value::Real(d)));
        frames.push((t, "duel32.玩家2.动作", kv::Value::Int(1)));
        frames.push((t, "duel32.玩家2.方向dx", kv::Value::Real(-d)));
        frames.push((t, "duel32.玩家2.方向dy", kv::Value::Real(-d)));
    }
    frames.push((9, "duel32.玩家1.动作", kv::Value::Int(3)));
    frames.push((9, "duel32.玩家1.方向dx", kv::Value::Real(d)));
    frames.push((9, "duel32.玩家1.方向dy", kv::Value::Real(d)));
    frames.push((9, "duel32.玩家1.射程", kv::Value::Real(6.627416997969522)));

    let r = kv::run_pack_inputs(&text, 12, &frames).expect("真对局应跑通");
    assert_eq!(
        kv::get(&r.snapshot, "duel32.场景.结果"),
        &serde_json::json!(1),
        "P1 应胜（结果=1）"
    );
    assert_eq!(
        kv::get(&r.snapshot, "duel32.玩家2.存活"),
        &serde_json::json!(false),
        "P2 应被命中"
    );
}

/// v0.3（听炸转正）：默认包内嵌"耳朵"——16 内真 · 边界 16.0 真（相切算中）· 16 外假。
#[test]
fn duel32_hearing_boundary_is_baked() {
    let text = kv::read_pack(DUEL);
    // 布景：t1 出生；t2 P1 发探（上，射程 6）＋ P2 发爆（右，射程 r）；t3 听炸判定。
    // 探器点（状态 2）＝出生点；距离 ＝ √(r²＋y0²)。
    let cases: [(f64, f64, bool); 3] = [
        (0.0, 15.9, true),  // 15.9 → 真
        (0.0, 16.0, true),  // 16.0 → 真（边界含）
        (6.0, 15.0, false), // √261 ≈ 16.155 → 假
    ];
    for (y0, r, want) in cases {
        let mut frames: Vec<(u64, &str, kv::Value)> = vec![];
        frames.push((1, "duel32.玩家1.出生x", kv::Value::Real(0.0)));
        frames.push((1, "duel32.玩家1.出生y", kv::Value::Real(y0)));
        frames.push((1, "duel32.玩家2.出生x", kv::Value::Real(0.0)));
        frames.push((1, "duel32.玩家2.出生y", kv::Value::Real(0.0)));
        frames.push((2, "duel32.玩家1.动作", kv::Value::Int(2)));
        frames.push((2, "duel32.玩家1.方向dx", kv::Value::Real(0.0)));
        frames.push((2, "duel32.玩家1.方向dy", kv::Value::Real(1.0)));
        frames.push((2, "duel32.玩家1.射程", kv::Value::Real(6.0)));
        frames.push((2, "duel32.玩家2.动作", kv::Value::Int(3)));
        frames.push((2, "duel32.玩家2.方向dx", kv::Value::Real(1.0)));
        frames.push((2, "duel32.玩家2.方向dy", kv::Value::Real(0.0)));
        frames.push((2, "duel32.玩家2.射程", kv::Value::Real(r)));
        let run = kv::run_pack_inputs(&text, 3, &frames).expect("应跑通");
        let heard = kv::get(&run.snapshot, "duel32.玩家1探1.听到").as_bool().unwrap_or(false);
        assert_eq!(heard, want, "y0={y0} r={r}：听炸应 want={want}");
        if want {
            let dx = kv::get(&run.snapshot, "duel32.玩家1探1.听dx").as_f64().unwrap_or(0.0);
            let dy = kv::get(&run.snapshot, "duel32.玩家1探1.听dy").as_f64().unwrap_or(0.0);
            let n = (dx * dx + dy * dy).sqrt();
            assert!((n - 1.0).abs() < 1e-9, "听向应归一：({dx},{dy})");
        } else {
            assert_eq!(kv::get(&run.snapshot, "duel32.玩家1探1.听dx").as_f64(), Some(0.0), "没听到＝0");
        }
    }
}
