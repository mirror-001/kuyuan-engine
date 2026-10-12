//! 库元引擎 · store 回归：存档／回放／回滚（《引擎·标准》§13；S8）。
//! 验收口径：「重演与原始运行**逐字节相等**」；三道校验该拒的拒；对账抓得住静默分裂。

use std::collections::BTreeMap;

use kuyuan_core::{value_of_json, InputFrame, InputScript, Pack, Value};
use kuyuan_driver::World;
use kuyuan_log::EventLog;
use kuyuan_verify as kv;

// ---------- 帮手 ----------

fn frames_of(rel: &str) -> Vec<InputFrame> {
    let s: InputScript = serde_json::from_str(&kv::read_pack(rel)).expect("输入集应能解析");
    s.frames
}

fn to_values(writes: &BTreeMap<String, serde_json::Value>) -> BTreeMap<String, Value> {
    writes
        .iter()
        .map(|(k, v)| (k.clone(), value_of_json(v).expect("输入值应合法")))
        .collect()
}

/// 跑一次"干净运行"（所有拍都被接受），输入集按 tick 取帧。
fn run(pack_text: &str, ticks: u64, frames: &[InputFrame]) -> (World, EventLog) {
    let pack: Pack = serde_json::from_str(pack_text).expect("包应能解析");
    let mut w = kuyuan_driver::load(&pack).expect("包应过门禁");
    let mut l = EventLog::new();
    for t in 1..=ticks {
        let mut writes: BTreeMap<String, Value> = BTreeMap::new();
        for f in frames.iter().filter(|f| f.tick == t) {
            writes.extend(to_values(&f.writes));
        }
        w.tick_in(t, &writes, &mut l).expect("干净运行不应被拒");
    }
    (w, l)
}

fn ev_text(log: &EventLog) -> String {
    log.events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// duel32 空跑 42 拍的一份现成档（多处测试共用）。
fn archive_duel32_empty42() -> (Vec<u8>, World, EventLog, kuyuan_store::Archive) {
    let bytes = kv::read_pack("32×32/duel32.json").into_bytes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let (w, l) = run(&text, 42, &[]);
    let a = kuyuan_store::save(&bytes, &[], &w, &l).expect("应能存档");
    (bytes, w, l, a)
}

// ---------- 正向 ----------

#[test]
fn duel32_empty_run_replays_byte_for_byte() {
    let (bytes, w0, l0, a) = archive_duel32_empty42();
    assert_eq!(a.tick, 42);
    assert_eq!(a.ns, "duel32");
    assert!(a.fingerprint.starts_with("fnv1a64:"));
    assert_eq!(l0.events.len(), 49, "42 拍＝49 条（41 变量变更 ＋ 8 搬运）");
    let (w1, l1) = kuyuan_store::replay(&bytes, &a).expect("应能回放");
    assert_eq!(w1.snapshot_json(), w0.snapshot_json(), "快照逐字节相等");
    assert_eq!(ev_text(&l1), ev_text(&l0), "事件流逐字节相等");
    assert_eq!(w1.tick, 42);
    assert_eq!(l1.last_tick, 42);
    kuyuan_store::reconcile(&w1, &l1).expect("对账应通过");
}

#[test]
fn duel32_real_match_replays_through_disk_roundtrip() {
    let bytes = kv::read_pack("32×32/duel32.json").into_bytes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let frames = frames_of("32×32/duel32.对局-01.json");
    let (w0, l0) = run(&text, 12, &frames);
    let a = kuyuan_store::save(&bytes, &frames, &w0, &l0).expect("应能存档");
    // 落盘 → 读回（跨盘形）
    let a_text = serde_json::to_string(&a).unwrap();
    let a2: kuyuan_store::Archive = serde_json::from_str(&a_text).unwrap();
    let (w1, l1) = kuyuan_store::replay(&bytes, &a2).expect("应能回放");
    assert_eq!(l1.events.len(), 329, "12 拍真对局＝329 条");
    assert_eq!(ev_text(&l1), ev_text(&l0));
    assert_eq!(w1.snapshot_json(), w0.snapshot_json());
    // 再回放一次＝确定性
    let (w2, _l2) = kuyuan_store::replay(&bytes, &a2).unwrap();
    assert_eq!(w2.snapshot_json(), w1.snapshot_json());
}

#[test]
fn archive_shape_is_fixed_and_has_no_view() {
    let (_, _, _, a) = archive_duel32_empty42();
    let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let o = v.as_object().unwrap();
    for k in ["标准", "包", "拍", "指纹", "输入", "封条"] {
        assert!(o.contains_key(k), "缺字段 {k}");
    }
    assert_eq!(o.len(), 6, "存档形状＝六个字段（不含视角名——裁 #36）");
    assert!(!o.contains_key("视角") && !o.contains_key("视角名") && !o.contains_key("事件"));
}

#[test]
fn non_finite_values_replay_through_archive() {
    let text = r#"{
  "标准": "库元/引擎/v0",
  "包": "nandemo",
  "版本": "0.1.0",
  "三问": { "可推翻": "任一拍快照中实数 v 不是自描述 NaN 即可推翻。", "论域": "单模块 · 一变量（开方负数）。", "检验者": "本测试。" },
  "模块": [ { "id": "nandemo.m",
    "变量": [ { "名": "v", "类型": "实数", "初值": 0 } ],
    "公式": [ { "名": "v.下一", "目标": "v", "表达式": ["开方", ["字面", -1]] } ] } ],
  "关系": []
}"#;
    let bytes = text.as_bytes().to_vec();
    let (w0, l0) = run(text, 2, &[]);
    let snap: serde_json::Value = serde_json::from_str(&w0.snapshot_json()).unwrap();
    assert_eq!(snap["nandemo.m.v"]["实数"], "NaN", "自描述形（§10·4：不写 null）");
    let a = kuyuan_store::save(&bytes, &[], &w0, &l0).expect("应能存档（含 NaN）");
    let (w1, l1) = kuyuan_store::replay(&bytes, &a).expect("应能回放（含 NaN）");
    assert_eq!(w1.snapshot_json(), w0.snapshot_json());
    assert_eq!(ev_text(&l1), ev_text(&l0));
}

#[test]
fn tick_is_not_part_of_snapshot() {
    let text = kv::read_pack("32×32/duel32.json");
    let (w, _) = run(&text, 3, &[]);
    assert_eq!(w.tick, 3, "世界「拍」随推进跟上（#42＝乙）");
    let snap: serde_json::Value = serde_json::from_str(&w.snapshot_json()).unwrap();
    let obj = snap.as_object().expect("快照应为对象");
    assert_eq!(obj.len(), w.addr_of.len(), "快照只含地址，不得多出「拍」等字段");
    assert!(obj.keys().all(|k| w.index.contains_key(k)), "快照键全为在场地址");
}

// ---------- 反例（该拒的拒） ----------

#[test]
fn fingerprint_rejects_changed_and_wrong_pack() {
    let (bytes, .., a) = archive_duel32_empty42();
    // 改一字节（追加空格）
    let mut bad = bytes.clone();
    bad.push(b' ');
    let e = kuyuan_store::replay(&bad, &a).err().expect("应拒");
    assert!(e.contains("指纹不符"), "{e}");
    // 换包（demo.计数器）
    let other = kv::read_pack("demo.计数器.json").into_bytes();
    let e = kuyuan_store::replay(&other, &a).err().expect("应拒");
    assert!(e.contains("指纹不符"), "{e}");
}

#[test]
fn archive_version_and_ns_are_checked() {
    let (bytes, .., a) = archive_duel32_empty42();
    let mut bad = a.clone();
    bad.std_version = "库元/引擎/存档/v99".into();
    let e = kuyuan_store::replay(&bytes, &bad).err().expect("应拒");
    assert!(e.contains("存档标准版本不认识"), "{e}");
    let mut bad = a.clone();
    bad.ns = "demo".into();
    let e = kuyuan_store::replay(&bytes, &bad).err().expect("应拒");
    assert!(e.contains("包名不符"), "{e}");
}

#[test]
fn seal_rejects_tampered_frames() {
    let bytes = kv::read_pack("32×32/duel32.json").into_bytes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let frames = frames_of("32×32/duel32.对局-01.json");
    let (w0, l0) = run(&text, 12, &frames);
    let mut a = kuyuan_store::save(&bytes, &frames, &w0, &l0).expect("应能存档");
    // 篡改第 9 拍：射程 6.627… → 3.0（仍合法，但落点不同 ⇒ 终态不同）
    let f = a.frames.iter_mut().find(|f| f.tick == 9).expect("应有第 9 拍帧");
    f.writes.insert("duel32.玩家1.射程".into(), serde_json::json!(3.0));
    let e = kuyuan_store::replay(&bytes, &a).err().expect("应拒");
    assert!(e.contains("封条不符"), "{e}");
}

#[test]
fn save_rejects_inconsistent_frames() {
    let bytes = kv::read_pack("32×32/duel32.json").into_bytes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let (w, l) = run(&text, 5, &[]);
    // 五拍空跑的世界 ＋ 第 1 拍"移动"的输入集——重演不出这个状态
    let mut writes: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    writes.insert("duel32.玩家1.动作".into(), serde_json::json!(1));
    writes.insert("duel32.玩家1.方向dx".into(), serde_json::json!(1.0));
    writes.insert("duel32.玩家1.方向dy".into(), serde_json::json!(0.0));
    let frames = vec![InputFrame { tick: 1, writes }];
    let e = kuyuan_store::save(&bytes, &frames, &w, &l).err().expect("应拒");
    assert!(e.contains("存档不自洽"), "{e}");
}

#[test]
fn save_requires_world_log_agreement() {
    let bytes = kv::read_pack("32×32/duel32.json").into_bytes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let (w, mut l) = run(&text, 3, &[]);
    l.last_tick = 5; // 手工分裂（模拟旧式"回滚账本"）
    let e = kuyuan_store::save(&bytes, &[], &w, &l).err().expect("应拒");
    assert!(e.contains("世界与账本不一致"), "{e}");
}

#[test]
fn reconcile_catches_silent_split() {
    let text = kv::read_pack("32×32/duel32.json");
    let (w, mut l) = run(&text, 3, &[]);
    kuyuan_store::reconcile(&w, &l).expect("正常应通过");
    l.last_tick = 5;
    let e = kuyuan_store::reconcile(&w, &l).err().expect("应拒");
    assert!(e.contains("世界与账本不一致") && e.contains("第 3 拍") && e.contains("第 5 拍"), "{e}");
}

// ---------- 回滚：分支 ＋ 重建 ----------

#[test]
fn branch_forks_without_touching_parent() {
    let bytes = kv::read_pack("32×32/duel32.json").into_bytes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    // 源运行：第 5 拍存档；原作续跑到第 10 拍
    let (w5, l5) = run(&text, 5, &[]);
    let a5 = kuyuan_store::save(&bytes, &[], &w5, &l5).expect("应能存档（第 5 拍）");
    let (_, l10) = run(&text, 10, &[]);
    let parent_before = ev_text(&l10);
    // 回滚：从第 5 拍另起分支（新账）
    let (mut wb, mut lb, br) = kuyuan_store::branch(&bytes, &a5).expect("应能分支");
    assert_eq!(br.from_tick, 5);
    assert_eq!(br.from_fingerprint, a5.fingerprint);
    assert_eq!(wb.tick, 5);
    assert_eq!(lb.last_tick, 5);
    assert!(lb.events.is_empty(), "新账只记此后");
    kuyuan_store::reconcile(&wb, &lb).expect("分支起手应一致");
    // 分支续跑第 6 拍（向左走一步，与原作分道）
    let mut writes: BTreeMap<String, Value> = BTreeMap::new();
    writes.insert("duel32.玩家1.动作".into(), Value::Int(1));
    writes.insert("duel32.玩家1.方向dx".into(), Value::Real(-1.0));
    writes.insert("duel32.玩家1.方向dy".into(), Value::Real(0.0));
    wb.tick_in(6, &writes, &mut lb).expect("分支应能续跑");
    assert_eq!(lb.last_tick, 6);
    assert!(!lb.events.is_empty());
    assert_eq!(lb.events[0].seq, 0, "新账流水号自 0——回避跳号（旧法截断会撞 [0..4,10]）");
    assert!(lb.events.iter().all(|e| e.tick == 6), "新账只见第 6 拍之后");
    kuyuan_store::reconcile(&wb, &lb).expect("续跑后仍一致");
    // 旧账一个字节不动
    assert_eq!(ev_text(&l10), parent_before);
    assert_eq!(l10.last_tick, 10);
}