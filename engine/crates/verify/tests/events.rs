//! 事件分类与拒绝入日志（S4）：《引擎·标准》§7.1 四类事件 · §7.2 拒绝事件 · §6／§10·7「记日志」。

use std::collections::BTreeMap;

use kuyuan_verify as kv;

const HEAD: &str = r#"{"标准":"库元/引擎/v0","包":"p1","版本":"0.1.0","三问":{"可推翻":"x","论域":"y","检验者":"z"},"#;

fn pack(body: &str) -> String {
    format!("{HEAD}{body}")
}

// ---------- §7.1 四类：搬运 ≠ 变量变更 ≠ 输入注入 ----------

/// 接线搬运记「搬运」；公式写回记「变量变更」；两者不得混同（S4 前搬运混在变量变更里）。
#[test]
fn carry_and_formula_are_classified() {
    let r = kv::run_pack(&kv::read_pack("demo.接线.json"), 2).expect("L1 应跑通");
    assert_eq!(r.events.len(), 4, "两拍四条事件");

    let carry: Vec<_> = r.events.iter().filter(|e| e.kind == "搬运").collect();
    assert_eq!(carry.len(), 1, "只有一条接线 → 一条搬运（t1 值未变不记）");
    assert_eq!(carry[0].target, "demo.加倍器.入值");
    assert_eq!(carry[0].tick, 2);
    assert_eq!((carry[0].old.clone(), carry[0].new.clone()), (serde_json::json!(0), serde_json::json!(1)));

    let varw: Vec<_> = r.events.iter().filter(|e| e.kind == "变量变更").collect();
    assert_eq!(varw.len(), 3, "公式写回三条（计数 ×2、倍值 ×1）");
    assert!(varw.iter().all(|e| e.target != "demo.加倍器.入值"), "输入不得由公式写");
    assert!(varw.iter().any(|e| e.target == "demo.加倍器.倍值"), "倍值由公式写");
}

/// 同一 tick 内的**确定序**：搬运 → 输入注入 → 公式写回（§7.1）。
#[test]
fn same_tick_order_is_carry_then_input_then_formula() {
    let text = kv::read_pack("32×32/duel32.json");
    let r = kv::run_pack_inputs(
        &text,
        1,
        &[
            (1, "duel32.玩家1.动作", kv::Value::Int(1)),
            (1, "duel32.玩家1.方向dx", kv::Value::Real(1.0)),
            (1, "duel32.玩家1.方向dy", kv::Value::Real(0.0)),
        ],
    )
    .expect("注入应跑通");
    let kinds: Vec<&str> = r.events.iter().filter(|e| e.tick == 1).map(|e| e.kind.as_str()).collect();
    let first_input = kinds.iter().position(|k| *k == "输入注入").expect("应有输入注入");
    let last_carry = kinds.iter().rposition(|k| *k == "搬运");
    if let Some(c) = last_carry {
        assert!(c < first_input, "搬运应排在输入注入之前，实得：{kinds:?}");
    }
    let first_var = kinds.iter().position(|k| *k == "变量变更");
    if let Some(v) = first_var {
        assert!(first_input < v, "输入注入应排在公式写回之前，实得：{kinds:?}");
    }
}

/// 值未变则不记（§7.1「旧≠新」）：注入与现值相同 → 无「输入注入」事件。
#[test]
fn unchanged_value_is_not_recorded() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"入","类型":"整数","默认":5}],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0}],"公式":[{"名":"v.下一","目标":"v","表达式":["引用","p1.m.入"]}]}],"关系":[]}"#,
    );
    // t1：注入 5（＝现值）→ 无注入事件；v 0→5 记一条变量变更
    let a = kv::run_pack_inputs(&t, 1, &[(1, "p1.m.入", kv::Value::Int(5))]).expect("应跑通");
    assert!(!a.events.iter().any(|e| e.kind == "输入注入"), "同值注入不该记事件");
    assert_eq!(a.events.len(), 1, "只有 v 的变量变更");
    assert_eq!(a.events[0].kind, "变量变更");
    // t2：注入 6（≠现值）→ 记一条输入注入
    let b = kv::run_pack_inputs(&t, 2, &[(2, "p1.m.入", kv::Value::Int(6))]).expect("应跑通");
    assert!(b.events.iter().any(|e| e.kind == "输入注入"), "异值注入应记事件");
}

// ---------- §7.2 拒绝入日志 ----------

/// 拒绝事件带**来源**字段（裁 #31 收口）：加载期＝`加载`（tick 0 · 目标＝包名）；
/// 运行期＝`运行期`——不再靠 `tick＝0` ＋ 目标＝包名**隐含**；其余事件**不写**此字段（故旧输出逐字节不变）。
#[test]
fn reject_events_carry_source_field() {
    // 加载期
    let bad = pack(
        r#""模块":[{"id":"p1.m","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":"文本"}],"公式":[{"名":"v.下一","目标":"v","表达式":["字面",1]}]}],"关系":[]}"#,
    );
    let (r, events) = kv::load_pack_logged(&bad);
    assert!(r.is_err(), "该拒");
    assert_eq!(events[0].source.as_deref(), Some("加载"));
    assert_eq!(
        serde_json::to_string(&events[0]).expect("应可序列化"),
        format!(
            r#"{{"序":0,"tick":0,"类型":"拒绝","目标":"p1","旧":null,"新":{},"来源":"加载"}}"#,
            serde_json::to_string(events[0].new.as_str().expect("理由")).expect("应可序列化")
        ),
        "形状：来源在尾部"
    );

    // 运行期
    let text = kv::read_pack("32×32/duel32.json");
    let p: kuyuan_core::Pack = serde_json::from_str(&text).expect("包应可解析");
    let mut w = kuyuan_driver::load(&p).expect("应加载");
    let mut log = kuyuan_log::EventLog::new();
    let mut frame: BTreeMap<String, kv::Value> = BTreeMap::new();
    frame.insert("duel32.场景.结果".to_string(), kv::Value::Int(1));
    w.tick_in(1, &frame, &mut log).err().expect("写变量应被拒");
    assert_eq!(log.events[0].source.as_deref(), Some("运行期"));

    // 状态事件不带来源
    let ok = kv::run_pack(&kv::read_pack("demo.计数器.json"), 1).expect("应跑通");
    assert!(ok.events.iter().all(|e| e.source.is_none()), "状态事件不写「来源」");
    let line = serde_json::to_string(&ok.events[0]).expect("应可序列化");
    assert!(!line.contains("来源"), "旧输出逐字节不变，实得：{line}");
}

/// 加载被拒 → 日志里有一条「拒绝」：目标＝包名，新＝理由原文，旧＝null，tick＝0。
#[test]
fn load_rejection_is_logged() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":"文本"}],"公式":[{"名":"v.下一","目标":"v","表达式":["字面",1]}]}],"关系":[]}"#,
    );
    let (r, events) = kv::load_pack_logged(&t);
    let e = r.err().expect("应被拒");
    assert!(e.contains("类型不符"), "实得：{e}");
    assert_eq!(events.len(), 1, "拒绝应记且只记一条");
    let ev = &events[0];
    assert_eq!(ev.kind, "拒绝");
    assert_eq!(ev.target, "p1", "加载期拒绝的目标＝包名");
    assert_eq!(ev.tick, 0, "加载期无 tick");
    assert!(ev.old.is_null(), "拒绝事件无旧值");
    assert_eq!(ev.new.as_str(), Some(e.as_str()), "理由原文入「新」");
}

/// 成功加载不记「拒绝」。
#[test]
fn successful_load_logs_nothing() {
    let (r, events) = kv::load_pack_logged(&kv::read_pack("demo.计数器.json"));
    assert!(r.is_ok(), "L0 应加载");
    assert!(events.is_empty(), "加载成功不该有事件");
}

/// 运行期拒绝入日志：写变量 → 拒，且日志留下那条拒绝（§6「非法输入一律拒绝并记日志」）。
#[test]
fn runtime_injection_rejection_is_logged() {
    let text = kv::read_pack("32×32/duel32.json");
    let p: kuyuan_core::Pack = serde_json::from_str(&text).expect("包应可解析");
    let mut w = kuyuan_driver::load(&p).expect("duel32 应加载");
    let mut log = kuyuan_log::EventLog::new();
    let mut frame: BTreeMap<String, kv::Value> = BTreeMap::new();
    frame.insert("duel32.场景.结果".to_string(), kv::Value::Int(1));

    let err = w.tick_in(1, &frame, &mut log).err().expect("写变量应被拒");
    assert!(err.contains("不是输入"), "实得：{err}");
    assert_eq!(log.events.len(), 1, "拒绝应记一条");
    let ev = &log.events[0];
    assert_eq!(ev.kind, "拒绝");
    assert_eq!(ev.target, "duel32.场景.结果", "运行期拒绝的目标＝该地址");
    assert_eq!(ev.tick, 1);
    assert!(ev.new.as_str().unwrap_or_default().contains("不是输入"), "理由入「新」");
    assert!(ev.old.is_null());
}

/// §10·7：整数除零＝拒绝**并记日志**（S4 前只有 Err，没有日志）。
#[test]
fn integer_division_by_zero_is_logged() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":1}],"公式":[{"名":"v.下一","目标":"v","表达式":["除",["引用","p1.m.v"],["字面",0]]}]}],"关系":[]}"#,
    );
    let p: kuyuan_core::Pack = serde_json::from_str(&t).expect("包应可解析");
    let mut w = kuyuan_driver::load(&p).expect("应加载（除零是运行期的事）");
    let mut log = kuyuan_log::EventLog::new();
    let err = w.tick(1, &mut log).err().expect("除零应被拒");
    assert!(err.contains("除零"), "实得：{err}");
    assert_eq!(log.events.len(), 1, "拒绝应记一条");
    assert_eq!(log.events[0].kind, "拒绝");
    assert_eq!(log.events[0].target, "p1.m.v", "目标＝该公式要写的地址");
    assert!(log.events[0].new.as_str().unwrap_or_default().contains("除零"));
}

// ---------- §10·3 拍序（留痕批）：留痕是拍序的保管者 ----------

/// **该拒的拒**：跳号与重复都不得推进——账本若对拍序沉默，"唯一事实源"就只是口号。
/// 两者都记一条带 `来源＝运行期` 的「拒绝」；且**被拒的拍不算发生过**（`last_tick` 不动）。
#[test]
fn tick_sequence_rejects_skip_and_repeat() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0}],"公式":[{"名":"v.下一","目标":"v","表达式":["加",["引用","p1.m.v"],["字面",1]]}]}],"关系":[]}"#,
    );
    let p: kuyuan_core::Pack = serde_json::from_str(&t).expect("包应可解析");
    let mut w = kuyuan_driver::load(&p).expect("应加载");
    let mut log = kuyuan_log::EventLog::new();

    // 跳号：首拍不是 1
    let e = w.tick(7, &mut log).err().expect("跳号应被拒");
    assert!(e.contains("拍序不符"), "实得：{e}");
    assert!(e.contains("期望第 1 拍") && e.contains("实得第 7 拍"), "措辞应点名两拍，实得：{e}");
    let ev = log.events.last().expect("拒绝应入账");
    assert_eq!(ev.kind, "拒绝");
    assert_eq!(ev.source.as_deref(), Some("运行期"));
    assert!(ev.new.as_str().unwrap_or_default().contains("拍序不符"));
    assert_eq!(log.last_tick, 0, "被拒的拍不算发生过");

    // 正常：第 1 拍过，账本时间前进
    w.tick(1, &mut log).expect("第 1 拍应跑通");
    assert_eq!(log.last_tick, 1);

    // 重复：同一拍再来一次
    let e = w.tick(1, &mut log).err().expect("重复应被拒");
    assert!(e.contains("期望第 2 拍，实得第 1 拍"), "实得：{e}");
    assert_eq!(log.last_tick, 1, "被拒的重复不推进账本时间");

    // 跳号：跳过第 2 拍
    let e = w.tick(5, &mut log).err().expect("跳号应被拒");
    assert!(e.contains("期望第 2 拍，实得第 5 拍"), "实得：{e}");
    assert_eq!(log.last_tick, 1);
}

/// **该过的过**：1、2、3 顺序推进——流水号连续、账本时间同步，且成功链路上一次都不拒。
#[test]
fn sequential_ticks_advance_and_number_contiguously() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0}],"公式":[{"名":"v.下一","目标":"v","表达式":["加",["引用","p1.m.v"],["字面",1]]}]}],"关系":[]}"#,
    );
    let p: kuyuan_core::Pack = serde_json::from_str(&t).expect("包应可解析");
    let mut w = kuyuan_driver::load(&p).expect("应加载");
    let mut log = kuyuan_log::EventLog::new();
    for tick in 1..=3u64 {
        w.tick(tick, &mut log).expect("顺序推进应跑通");
        assert_eq!(log.last_tick, tick, "账本时间应跟上");
    }
    assert_eq!(log.events.len(), 3, "三拍三条变量变更");
    assert!(!log.events.iter().any(|e| e.kind == "拒绝"), "顺序推进不该有拒绝");
    let seqs: Vec<u64> = log.events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![0, 1, 2], "流水号自 0 连续（重编号的基准）");
    assert_eq!(w.get("p1.m.v"), Some(&kv::Value::Int(3)), "三拍后 v＝3");
}

// ---------- §7 事实源：初值 ＋ 事件流 ＝ 终态（D5） ----------

/// 重演：把事件流按序贴回「初值快照」，应逐字段等于终态快照——**且每条事件的「旧」须等于当时状态**。
/// （这是 §7「状态可由初值＋事件流重演」的实测；S8 存档／回放的地基。）
#[test]
fn events_replay_to_final_snapshot() {
    let text = kv::read_pack("32×32/duel32.json");
    let p: kuyuan_core::Pack = serde_json::from_str(&text).expect("包应可解析");
    let mut w = kuyuan_driver::load(&p).expect("duel32 应加载");
    let t0: serde_json::Value = serde_json::from_str(&w.snapshot_json()).expect("初值快照");
    let mut log = kuyuan_log::EventLog::new();
    for t in 1..=42 {
        w.tick(t, &mut log).expect("应跑通");
    }
    let t_end: serde_json::Value = serde_json::from_str(&w.snapshot_json()).expect("终态快照");

    let mut replay = t0.clone();
    let mut applied = 0usize;
    for e in &log.events {
        if e.kind == "拒绝" {
            continue;
        }
        let cur = replay.get(e.target.as_str()).cloned().unwrap_or(serde_json::Value::Null);
        assert_eq!(cur, e.old, "事件「旧」应等于当时状态：{} @t{}", e.target, e.tick);
        replay[e.target.as_str()] = e.new.clone();
        applied += 1;
    }
    assert_eq!(applied, log.events.len(), "无拒绝事件（本例不该有）");
    assert_eq!(replay, t_end, "初值＋事件流应重演出终态");
}
