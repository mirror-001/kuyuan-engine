//! 实数非有限值（S6 落 · 收口改）：《引擎·标准》§10·4——**自描述形** `{"实数":"NaN"}`（裁 #32＝甲）·
//! 日志按表示比较 · 反解无损 · IEEE 语义不变。

use kuyuan_verify as kv;

const HEAD: &str = r#"{"标准":"库元/引擎/v0","包":"p1","版本":"0.1.0","三问":{"可推翻":"x","论域":"y","检验者":"z"},"#;

/// 非有限实数的**自描述形**（§10·4；裁 #32＝甲）：结构化，与文本变量里的 `"NaN"` 不再同形。
fn marked(m: &str) -> serde_json::Value {
    serde_json::json!({ "实数": m })
}

/// 单变量包：`v`（实数，初值 0.0）每拍变成本表达式之值。
fn real_pack(expr: &str) -> String {
    format!(
        r#"{HEAD}"模块":[{{"id":"p1.m","输入":[],"输出":[],"变量":[{{"名":"v","类型":"实数","初值":0.0}}],"公式":[{{"名":"v.下一","目标":"v","表达式":{expr}}}]}}],"关系":[]}}"#
    )
}

/// `0.0/0.0` → NaN 应写成 `{"实数":"NaN"}`，**不是** `null`，也不是裸串。
#[test]
fn nan_is_marked_not_null() {
    let t = real_pack(r#"["除",["字面",0.0],["字面",0.0]]"#);
    let r = kv::run_pack(&t, 1).expect("应跑通");
    assert_eq!(kv::get(&r.snapshot, "p1.m.v"), &marked("NaN"), "快照应写自描述标记");
    assert_eq!(r.events.len(), 1);
    assert_eq!(r.events[0].new, marked("NaN"), "事件「新」应写自描述标记");
    assert_ne!(r.events[0].new, serde_json::json!(null), "不得静默成 null");
    assert_ne!(r.events[0].new, serde_json::json!("NaN"), "不再是裸串（会与文本变量同形）");
}

/// `1.0/0.0` → `{"实数":"inf"}`；`-1.0/0.0` → `{"实数":"-inf"}`。
#[test]
fn infinities_are_marked() {
    let pos = kv::run_pack(&real_pack(r#"["除",["字面",1.0],["字面",0.0]]"#), 1).expect("应跑通");
    assert_eq!(kv::get(&pos.snapshot, "p1.m.v"), &marked("inf"));
    let neg = kv::run_pack(&real_pack(r#"["除",["字面",-1.0],["字面",0.0]]"#), 1).expect("应跑通");
    assert_eq!(kv::get(&neg.snapshot, "p1.m.v"), &marked("-inf"));
}

/// `开方(-1)` → `{"实数":"NaN"}`。
#[test]
fn sqrt_of_negative_is_nan() {
    let r = kv::run_pack(&real_pack(r#"["开方",["字面",-1.0]]"#), 1).expect("应跑通");
    assert_eq!(kv::get(&r.snapshot, "p1.m.v"), &marked("NaN"));
}

/// **不再刷假事件**：`NaN` 对 `NaN` 不算变化（§10·4 按表示比较）——S6 前每拍刷一条 `null→null`。
#[test]
fn nan_does_not_spam_events() {
    let r = kv::run_pack(&real_pack(r#"["除",["字面",0.0],["字面",0.0]]"#), 5).expect("应跑通");
    assert_eq!(r.events.len(), 1, "五拍只该记「0.0 → NaN」一条，实得：{:?}",
        r.events.iter().map(|e| (&e.tick, &e.old, &e.new)).collect::<Vec<_>>());
    assert_eq!(r.events[0].old, serde_json::json!(0.0));
    assert_eq!(r.events[0].new, marked("NaN"));
    // inf 同理：五拍也只记一条
    let ri = kv::run_pack(&real_pack(r#"["除",["字面",1.0],["字面",0.0]]"#), 5).expect("应跑通");
    assert_eq!(ri.events.len(), 1, "inf 稳定后不该再记");
}

/// IEEE 语义**未动**（§10·7）：`inf - inf` → NaN；`NaN < 1` 为假 → 走「否」分支。
#[test]
fn ieee_semantics_unchanged() {
    let r = kv::run_pack(
        &real_pack(r#"["减",["除",["字面",1.0],["字面",0.0]],["除",["字面",1.0],["字面",0.0]]]"#),
        1,
    )
    .expect("应跑通");
    assert_eq!(kv::get(&r.snapshot, "p1.m.v"), &marked("NaN"), "inf−inf＝NaN");

    let t = format!(
        r#"{HEAD}"模块":[{{"id":"p1.m","输入":[],"输出":[],"变量":[{{"名":"b","类型":"布尔","初值":false}}],"公式":[{{"名":"b.下一","目标":"b","表达式":["若",["小于",["除",["字面",0.0],["字面",0.0]],["字面",1.0]],["字面",true],["字面",false]]}}]}}],"关系":[]}}"#
    );
    let rb = kv::run_pack(&t, 1).expect("应跑通");
    assert_eq!(
        kv::get(&rb.snapshot, "p1.m.b"),
        &serde_json::json!(false),
        "NaN < 1 应为假（IEEE）"
    );
}

/// 标记**反解无损**（§10·4）：`real_marker` 与 `real_of_marker` 成对可逆；非标记串不认。
#[test]
fn markers_round_trip() {
    use kuyuan_core::{real_marker, real_of_marker};
    assert!(real_of_marker(real_marker(f64::NAN)).expect("应反解").is_nan());
    assert_eq!(real_of_marker(real_marker(f64::INFINITY)), Some(f64::INFINITY));
    assert_eq!(real_of_marker(real_marker(f64::NEG_INFINITY)), Some(f64::NEG_INFINITY));
    assert_eq!(real_of_marker("nan"), None, "只认规范拼法");
    assert_eq!(real_of_marker("Infinity"), None);
    assert_eq!(real_of_marker("1.5"), None, "有限值不走标记");
}

/// **JSON 层也闭环**（裁 #32＝甲）：`值 → JSON → 值` 无损，读取端**无须**类型表。
#[test]
fn marked_json_round_trips_to_real() {
    use kuyuan_core::{json_of_value, value_of_json, Value};
    for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let j = json_of_value(&Value::Real(f));
        assert!(j.is_object(), "非有限实数应走自描述形，实得：{j}");
        let back = value_of_json(&j).expect("应能反解");
        match back {
            Value::Real(g) => assert_eq!(g.is_nan(), f.is_nan(), "NaN 语义保住"),
            other => panic!("应反解为实数，得：{other:?}"),
        }
    }
    assert_eq!(
        value_of_json(&serde_json::json!({ "实数": "1.5" })),
        Err("字面类型未支持: {\"实数\":\"1.5\"}".to_string()),
        "只认标记，不认别的对象"
    );
    assert_eq!(
        value_of_json(&serde_json::json!("NaN")).expect("裸串照旧是文本"),
        Value::Text("NaN".to_string())
    );
}

/// 非有限值也能**重演**（§7 D5）：初值 ＋ 事件流 ＝ 终态，NaN 在场时照样成立。
#[test]
fn non_finite_replays_to_final_snapshot() {
    let t = real_pack(r#"["除",["字面",0.0],["字面",0.0]]"#);
    let p: kuyuan_core::Pack = serde_json::from_str(&t).expect("包应可解析");
    let mut w = kuyuan_driver::load(&p).expect("应加载");
    let t0: serde_json::Value = serde_json::from_str(&w.snapshot_json()).expect("初值快照");
    let mut log = kuyuan_log::EventLog::new();
    for tick in 1..=5 {
        w.tick(tick, &mut log).expect("应跑通");
    }
    let t_end: serde_json::Value = serde_json::from_str(&w.snapshot_json()).expect("终态快照");
    let mut replay = t0.clone();
    for e in &log.events {
        assert_eq!(replay.get(e.target.as_str()).unwrap(), &e.old, "「旧」应等于当时状态");
        replay[e.target.as_str()] = e.new.clone();
    }
    assert_eq!(replay, t_end, "含 NaN 时应仍能重演");
    assert_eq!(t_end["p1.m.v"], marked("NaN"));
    // 重演出来的标记还能读回实数（存档 S8 的入口）
    assert!(kuyuan_core::value_of_json(&t_end["p1.m.v"])
        .expect("应反解")
        .as_f64()
        .expect("应是数值")
        .is_nan());
}

/// **旧歧义已销**（裁 #32＝甲）：文本变量的 `"NaN"` 与实数 NaN **不同形**——不再需要类型表。
/// （S6 时两者同为裸串 `"NaN"`，此测当时锁的是"同形"；本批有意改为锁"可分辨"。）
#[test]
fn text_nan_and_real_nan_are_distinguishable() {
    let t = format!(
        r#"{HEAD}"模块":[{{"id":"p1.m","输入":[],"输出":[],"变量":[{{"名":"t","类型":"文本","初值":"NaN"}},{{"名":"r","类型":"实数","初值":0.0}}],"公式":[{{"名":"r.下一","目标":"r","表达式":["除",["字面",0.0],["字面",0.0]]}}]}}],"关系":[]}}"#
    );
    let r = kv::run_pack(&t, 1).expect("应跑通");
    assert_eq!(kv::get(&r.snapshot, "p1.m.t"), &serde_json::json!("NaN"), "文本变量照原样写");
    assert_eq!(kv::get(&r.snapshot, "p1.m.r"), &marked("NaN"), "实数 NaN 写自描述形");
    assert_ne!(
        kv::get(&r.snapshot, "p1.m.t"),
        kv::get(&r.snapshot, "p1.m.r"),
        "两者在 JSON 里已可分辨（#32 已裁＝甲）"
    );
}
