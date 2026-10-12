//! 守护（四界）回归（S9）：《引擎·标准》§14——注入两道（类型 §14·2 · 界式 §14·3）
//! ＋ 时间界（拍上限 §14·5）。
//! 正反成对；措辞只断**前缀词**（全句空格约定，逐字节断言留给库外冒烟与基准复核）。

use std::collections::BTreeMap;

use kuyuan_log::EventLog;
use kuyuan_verify as kv;

use kv::{Event, Value};

/// 包骨架：三问齐备，后接 `模块`／`关系` 段（与 gates／events 同式）。
const HEAD: &str = r#"{"标准":"库元/引擎/v0","包":"p1","版本":"0.1.0","三问":{"可推翻":"x","论域":"y","检验者":"z"},"#;

fn pack(body: &str) -> String {
    format!("{HEAD}{body}")
}

/// 一帧注入（`(地址, 值)`）。
fn frame(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs.iter().map(|(a, v)| ((*a).to_string(), v.clone())).collect()
}

/// 事件流里的「拒绝」。
fn rejects(evs: &[Event]) -> Vec<&Event> {
    evs.iter().filter(|e| e.kind == "拒绝").collect()
}

/// 带「界」的包：`值` 挂 `值 ≥ 0`；`累计` 每拍加 `值`。
fn bound_pack() -> String {
    pack(r#""模块":[{"id":"p1.m","输入":[{"名":"值","类型":"整数","默认":0,"界":["大于等于",["引用","p1.m.值"],["字面",0]]}],"变量":[{"名":"累计","类型":"整数","初值":0}],"公式":[{"名":"累计.下一","目标":"累计","表达式":["加",["引用","p1.m.累计"],["引用","p1.m.值"]]}]}],"关系":[]}"#)
}

// ---------- 正 ----------

/// 正：界式真 ⇒ 注入照常（`输入注入` 在 · 世界更新 · 无拒绝）。
#[test]
fn bound_true_passes() {
    let t = bound_pack();
    let mut w = kv::load_pack(&t).expect("含「界」的包应加载通过");
    let mut log = EventLog::new();
    w.tick_in(1, &frame(&[("p1.m.值", Value::Int(5))]), &mut log)
        .expect("界真应放行");

    assert_eq!(log.events.iter().filter(|e| e.kind == "输入注入").count(), 1, "注入应记痕");
    assert!(rejects(&log.events).is_empty(), "不应有拒绝");
    let acc = log
        .events
        .iter()
        .find(|e| e.kind == "变量变更" && e.target == "p1.m.累计")
        .expect("累计应有变量变更");
    assert_eq!(acc.new, serde_json::json!(5));
    assert_eq!(w.get("p1.m.值"), Some(&Value::Int(5)), "世界应更新");
}

/// 正：未声明「界」＝无界（旧包行为）——越界值照收。
#[test]
fn unbound_is_unbounded() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"值","类型":"整数","默认":0}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let mut log = EventLog::new();
    w.tick_in(1, &frame(&[("p1.m.值", Value::Int(-1))]), &mut log)
        .expect("无界应放行");
    assert!(rejects(&log.events).is_empty());
    assert_eq!(w.get("p1.m.值"), Some(&Value::Int(-1)));
}

/// 正反成对：整数可入实数（放行）；实数入整数＝拒（§3 相容律）。
#[test]
fn int_into_real_ok_real_into_int_rejected() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"数","类型":"实数","默认":0}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let mut log = EventLog::new();
    w.tick_in(1, &frame(&[("p1.m.数", Value::Int(5))]), &mut log)
        .expect("整数入实数应放行");
    assert_eq!(w.get("p1.m.数"), Some(&Value::Int(5)));

    let t2 = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"数","类型":"整数","默认":0}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let mut w2 = kv::load_pack(&t2).expect("应加载通过");
    let mut log2 = EventLog::new();
    let e = w2
        .tick_in(1, &frame(&[("p1.m.数", Value::Real(1.5))]), &mut log2)
        .expect_err("实数入整数应拒");
    assert!(e.contains("注入类型不符"), "实得：{e}");
}

// ---------- 反 ----------

/// 反：类型不符 ⇒ **先记「拒绝」再 Err**；世界与账本一个字节不动。
#[test]
fn type_mismatch_rejected_before_apply() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"数","类型":"整数","默认":0}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let before = w.snapshot_json();
    let mut log = EventLog::new();
    let e = w
        .tick_in(1, &frame(&[("p1.m.数", Value::Text("文本".into()))]), &mut log)
        .expect_err("应拒");
    assert!(e.contains("注入类型不符"), "实得：{e}");

    let rj = rejects(&log.events);
    assert_eq!(rj.len(), 1, "先记一条拒绝");
    assert_eq!(rj[0].source.as_deref(), Some("运行期"));
    assert_eq!(rj[0].target, "p1.m.数");
    assert!(rj[0].new.as_str().unwrap_or_default().contains("拒绝注入"));
    assert_eq!(w.snapshot_json(), before, "世界不动");
    assert_eq!(w.tick, 0);
    assert_eq!(log.last_tick, 0, "被拒的拍不算发生过");
}

/// 反：界式假（`-1` 犯 `≥ 0`）⇒ `越界注入`；世界不动。
#[test]
fn bound_false_rejected() {
    let t = bound_pack();
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let before = w.snapshot_json();
    let mut log = EventLog::new();
    let e = w
        .tick_in(1, &frame(&[("p1.m.值", Value::Int(-1))]), &mut log)
        .expect_err("应拒");
    assert!(e.contains("越界注入"), "实得：{e}");
    let rj = rejects(&log.events);
    assert_eq!(rj.len(), 1);
    assert_eq!(rj[0].target, "p1.m.值");
    assert_eq!(w.snapshot_json(), before);
    assert_eq!(log.last_tick, 0);
}

/// 反：界式**求值失败**（界里含 1／0）⇒ `界式被拒`（同形）。
#[test]
fn bound_eval_error_rejected() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"值","类型":"整数","默认":0,"界":["小于",["除",["字面",1],["字面",0]],["字面",2]]}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let mut w = kv::load_pack(&t).expect("静推应过（除：整数／整数）");
    let mut log = EventLog::new();
    let e = w
        .tick_in(1, &frame(&[("p1.m.值", Value::Int(1))]), &mut log)
        .expect_err("求值失败应拒");
    assert!(e.contains("界式被拒"), "实得：{e}");
    assert_eq!(rejects(&log.events).len(), 1);
}

/// 反：**只报第一处**——两道分遍（先类型全帧、后界式），各按地址升序。
#[test]
fn first_offender_is_deterministic() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"a","类型":"整数","默认":0,"界":["大于等于",["引用","p1.m.a"],["字面",0]]},{"名":"z","类型":"整数","默认":0}],"变量":[],"公式":[]}],"关系":[]}"#,
    );

    // ① 类型遍先于界式遍：z 的类型错（a 的界还轮不到）。
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let mut log = EventLog::new();
    let e = w
        .tick_in(
            1,
            &frame(&[("p1.m.a", Value::Int(-1)), ("p1.m.z", Value::Text("文".into()))]),
            &mut log,
        )
        .expect_err("应拒");
    assert!(e.contains("注入类型不符"), "实得：{e}");
    assert_eq!(rejects(&log.events)[0].target, "p1.m.z", "类型遍先报");

    // ② 皆过类型后，界式遍按地址升序：a 先。
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let mut log = EventLog::new();
    let e = w
        .tick_in(
            1,
            &frame(&[("p1.m.a", Value::Int(-1)), ("p1.m.z", Value::Int(5))]),
            &mut log,
        )
        .expect_err("应拒");
    assert!(e.contains("越界注入"), "实得：{e}");
    assert_eq!(rejects(&log.events)[0].target, "p1.m.a");

    // ③ 两笔同错：按地址升序报 a，且一次只报一处。
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let mut log = EventLog::new();
    let _ = w
        .tick_in(
            1,
            &frame(&[
                ("p1.m.a", Value::Text("文".into())),
                ("p1.m.z", Value::Text("文".into())),
            ]),
            &mut log,
        )
        .expect_err("应拒");
    let rj = rejects(&log.events);
    assert_eq!(rj.len(), 1, "一次只报一处");
    assert_eq!(rj[0].target, "p1.m.a");
}

/// 界式读「本拍初值」＝ tick 初值 ＋ 本帧全部注入（§11·5）——a 的界看得见同帧的 b。
#[test]
fn bound_reads_frame_start_values() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"a","类型":"整数","默认":0,"界":["大于等于",["引用","p1.m.b"],["字面",0]]},{"名":"b","类型":"整数","默认":0}],"变量":[],"公式":[]}],"关系":[]}"#,
    );

    let mut w = kv::load_pack(&t).expect("应加载通过");
    let mut log = EventLog::new();
    let e = w
        .tick_in(
            1,
            &frame(&[("p1.m.a", Value::Int(1)), ("p1.m.b", Value::Int(-5))]),
            &mut log,
        )
        .expect_err("应拒");
    assert!(e.contains("越界注入"), "a 的界应看见同帧注入的 b，实得：{e}");
    assert_eq!(rejects(&log.events)[0].target, "p1.m.a");

    let mut w2 = kv::load_pack(&t).expect("应加载通过");
    let mut log2 = EventLog::new();
    w2.tick_in(
        1,
        &frame(&[("p1.m.a", Value::Int(1)), ("p1.m.b", Value::Int(5))]),
        &mut log2,
    )
    .expect("b 合规 → 两笔皆放行");
    assert!(rejects(&log2.events).is_empty());
    assert_eq!(w2.get("p1.m.b"), Some(&Value::Int(5)));
}

/// 反：界式的**加载期门禁**——须布尔（§12·19 顺延）；顺带：未知标签／引用在场。
#[test]
fn bound_load_gates() {
    // 须布尔：整数表达式 ⇒ 拒（记「加载」拒绝）。
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"值","类型":"整数","默认":0,"界":["加",["字面",1],["字面",1]]}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let (r, evs) = kv::load_pack_logged(&t);
    let e = r.err().expect("应拒");
    assert!(e.contains("类型不符") && e.contains("布尔"), "实得：{e}");
    let rj = rejects(&evs);
    assert_eq!(rj.len(), 1);
    assert_eq!(rj[0].source.as_deref(), Some("加载"), "加载期拒绝");

    // 未知标签 ⇒ 拒。
    let t2 = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"值","类型":"整数","默认":0,"界":["平方",["引用","p1.m.值"]]}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let e2 = kv::load_pack(&t2).err().expect("应拒");
    assert!(e2.contains("未知标签"), "实得：{e2}");

    // 引用不在场 ⇒ 拒。
    let t3 = pack(
        r#""模块":[{"id":"p1.m","输入":[{"名":"值","类型":"整数","默认":0,"界":["非",["引用","p1.m.无此址"]]}],"变量":[],"公式":[]}],"关系":[]}"#,
    );
    let e3 = kv::load_pack(&t3).err().expect("应拒");
    assert!(e3.contains("引用不存在"), "实得：{e3}");
}

// ---------- 时间界（拍上限） ----------

/// 时间界（§14·5）：上限内放行；超限＝**先记「拒绝」再 Err**（目标＝`拍` · 来源＝运行期）。
#[test]
fn tick_cap_check_limit() {
    let mut log = EventLog::new();
    assert!(kuyuan_guard::check_limit(3, 3, &mut log).is_ok(), "上限内放行");
    assert!(log.events.is_empty(), "放行不记痕");

    let e = kuyuan_guard::check_limit(4, 3, &mut log).expect_err("超限应拒");
    assert!(e.contains("拍上限已达") && e.contains("拒绝推进"), "实得：{e}");
    let rj = rejects(&log.events);
    assert_eq!(rj.len(), 1);
    assert_eq!(rj[0].source.as_deref(), Some("运行期"));
    assert_eq!(rj[0].target, "拍");
    assert_eq!(rj[0].tick, 4);
}

/// 时间界·接进运行：声明上限 ⇒ 第 N＋1 拍推进被拒，世界停在 N；缺省＝无界。
#[test]
fn tick_cap_wired_run() {
    let t = pack(
        r#""模块":[{"id":"p1.m","输入":[],"变量":[{"名":"v","类型":"整数","初值":0}],"公式":[{"名":"v.下一","目标":"v","表达式":["加",["引用","p1.m.v"],["字面",1]]}]}],"关系":[]}"#,
    );
    let mut w = kv::load_pack(&t).expect("应加载通过");
    let mut log = EventLog::new();
    let cap = 3u64;
    let mut refused: Option<String> = None;
    for tick in 1..=5 {
        if let Err(e) = kuyuan_guard::check_limit(tick, cap, &mut log) {
            refused = Some(e);
            break;
        }
        w.tick_in(tick, &frame(&[]), &mut log).expect("上限内应推进");
    }
    let e = refused.expect("第 4 拍应被拒");
    assert!(e.contains("拍上限已达"), "实得：{e}");
    assert_eq!(w.tick, 3, "世界停在 N");
    assert_eq!(log.last_tick, 3, "账本停在 N");
    assert_eq!(rejects(&log.events)[0].tick, 4);

    // 缺省＝无界：同一包跑 5 拍（不调用 check_limit）照常。
    let r = kv::run_pack(&t, 5).expect("无界应照常");
    assert!(!r.events.iter().any(|e| e.kind == "拒绝"));
}