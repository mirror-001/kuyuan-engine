//! 界 / 视角投影（S7）：《引擎·标准》§10·5——三档可见性 · 视角投影 · 投影不改世界 · 三条加载期校验。
//!
//! 验收头条：**视角看不到对方私有**。

use kuyuan_core::JUDGE;
use kuyuan_verify as kv;

const HEAD: &str = r#"{"标准":"库元/引擎/v0","包":"p1","版本":"0.1.0","三问":{"可推翻":"x","论域":"y","检验者":"z"},"#;

/// 一个「每拍 +1」的模块；`fields` ＝ 附加的模块字段（如 `,"可见性":"私","归属":"甲"`）。
fn counter(id: &str, fields: &str) -> String {
    format!(
        r#"{{"id":"{id}"{fields},"输入":[],"输出":[],"变量":[{{"名":"v","类型":"整数","初值":0}}],"公式":[{{"名":"v.下一","目标":"v","表达式":["加",["引用","{id}.v"],["字面",1]]}}]}}"#
    )
}

/// 四模块小包（各有一个每拍 +1 的变量）：
/// `p1.公`＝真（缺省）· `p1.甲`＝私·归属甲 · `p1.乙`＝私·归属乙 · `p1.密`＝裁判·归属甲（归属者自己也看不到）。
fn tiered_pack() -> String {
    format!(
        r#"{HEAD}"模块":[{},{},{},{}],"关系":[]}}"#,
        counter("p1.公", ""),
        counter("p1.甲", r#","可见性":"私","归属":"甲""#),
        counter("p1.乙", r#","可见性":"私","归属":"乙""#),
        counter("p1.密", r#","可见性":"裁判","归属":"甲""#),
    )
}

/// 跑 N 拍，返回（世界, 全量事件流）。
fn run(t: &str, ticks: u64) -> (kuyuan_driver::World, Vec<kuyuan_log::Event>) {
    let mut w = kv::load_pack(t).expect("应加载");
    let mut log = kuyuan_log::EventLog::new();
    for tick in 1..=ticks {
        w.tick(tick, &mut log).expect("应跑通");
    }
    (w, log.events)
}

/// 真档（缺省）：人人可见——旧包一个字段不改，也不会被藏起来。
#[test]
fn open_level_is_visible_to_everyone() {
    let (w, _) = run(&tiered_pack(), 1);
    for view in ["甲", "乙", "路人", JUDGE] {
        assert!(
            kuyuan_view::snapshot(&w, view).get("p1.公.v").is_some(),
            "真档应对「{view}」可见"
        );
    }
    // 旧包（无任何可见性声明）＝全真：路人视角也看得到全部地址
    let (d, _) = run(&kv::read_pack("demo.计数器.json"), 1);
    assert_eq!(
        kuyuan_view::visible_count(&d, "路人"),
        d.addr_of.len(),
        "未声明即可见——旧包行为不受 S7 影响"
    );
}

/// **验收头条**：私档＝归属者＋裁判；对方看不到。
#[test]
fn private_level_hides_from_other_players() {
    let (w, _) = run(&tiered_pack(), 1);
    let jia = kuyuan_view::snapshot(&w, "甲");
    let yi = kuyuan_view::snapshot(&w, "乙");
    assert!(jia.get("p1.甲.v").is_some(), "归属者应看见自己的私档");
    assert!(yi.get("p1.乙.v").is_some());
    assert!(yi.get("p1.甲.v").is_none(), "乙不得看见甲的私有");
    assert!(jia.get("p1.乙.v").is_none(), "甲不得看见乙的私有");
    assert!(kuyuan_view::snapshot(&w, JUDGE).get("p1.甲.v").is_some(), "裁判应看见私档");
}

/// 裁判档：**仅**裁判——连归属者自己也看不到。
#[test]
fn judge_level_is_judge_only() {
    let (w, _) = run(&tiered_pack(), 1);
    assert!(kuyuan_view::snapshot(&w, "甲").get("p1.密.v").is_none(), "归属者也不行");
    assert!(kuyuan_view::snapshot(&w, "乙").get("p1.密.v").is_none());
    assert!(kuyuan_view::snapshot(&w, JUDGE).get("p1.密.v").is_some());
}

/// 不认识的视角名：**不报错**，只余真档（视角不是世界的一部分）。
#[test]
fn unknown_view_name_sees_only_open() {
    let (w, _) = run(&tiered_pack(), 1);
    let s = kuyuan_view::snapshot(&w, "查无此人");
    assert_eq!(kuyuan_view::visible_count(&w, "查无此人"), 1, "只该剩真档那一个地址");
    assert!(s.get("p1.公.v").is_some());
    assert!(s.get("p1.甲.v").is_none() && s.get("p1.密.v").is_none());
}

/// 裁判视角 ＝ 全见：与 `World::snapshot_json` **逐字节**相等（故旧输出不变）。
#[test]
fn judge_view_equals_full_snapshot() {
    let (w, _) = run(&tiered_pack(), 3);
    assert_eq!(kuyuan_view::snapshot_json(&w, JUDGE), w.snapshot_json(), "裁判＝全见");
    assert_eq!(kuyuan_view::visible_count(&w, JUDGE), w.addr_of.len());
}

/// 事件流按档过滤：不可见地址的事件**整条不出现**（只藏值仍会泄漏"何时变过"）。
#[test]
fn events_are_filtered_whole() {
    let t = tiered_pack();
    let (w, full) = run(&t, 3);
    assert_eq!(full.len(), 12, "四模块 × 三拍");
    let jia = kuyuan_view::events(&w, &full, "甲");
    let targets: Vec<&str> = jia.iter().map(|e| e.target.as_str()).collect();
    assert!(
        !targets.iter().any(|a| a.ends_with("乙.v") || a.ends_with("密.v")),
        "乙／密的事件不该出现，实得：{targets:?}"
    );
    assert_eq!(jia.len(), 6, "甲视角：公＋甲 各三拍");
    assert_eq!(kuyuan_view::events(&w, &full, JUDGE).len(), 12);
}

/// 投影内**重编号**：`序` 自 0 连续——留空号会泄漏"隐藏地址变过几次、在第几拍"。
#[test]
fn projected_events_are_renumbered() {
    let t = tiered_pack();
    let (w, full) = run(&t, 3);
    let jia = kuyuan_view::events(&w, &full, "甲");
    let seqs: Vec<u64> = jia.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![0, 1, 2, 3, 4, 5], "投影内序应自 0 连续");
    let full_seqs: Vec<u64> = full.iter().map(|e| e.seq).collect();
    assert_eq!(full_seqs, (0..12).collect::<Vec<u64>>(), "原始日志不动（D5）");
    assert_eq!(w.snapshot_json().matches("p1.乙.v").count(), 1, "世界照常演算");
}

/// 输入随**模块默认**：私模块的入口，对方看不到；注入事件同样被滤掉。
#[test]
fn input_follows_module_default() {
    let t = format!(
        r#"{HEAD}"模块":[{{"id":"p1.甲","可见性":"私","归属":"甲","输入":[{{"名":"入","类型":"整数","默认":0}}],"输出":[],"变量":[{{"名":"v","类型":"整数","初值":0}}],"公式":[{{"名":"v.下一","目标":"v","表达式":["引用","p1.甲.入"]}}]}}],"关系":[]}}"#
    );
    let mut w = kv::load_pack(&t).expect("应加载");
    let mut log = kuyuan_log::EventLog::new();
    let mut frame = std::collections::BTreeMap::new();
    frame.insert("p1.甲.入".to_string(), kv::Value::Int(7));
    w.tick_in(1, &frame, &mut log).expect("应跑通");

    assert!(kuyuan_view::snapshot(&w, "甲").get("p1.甲.入").is_some());
    assert!(kuyuan_view::snapshot(&w, "乙").get("p1.甲.入").is_none(), "入口随模块默认＝私");
    let yi_events = kuyuan_view::events(&w, &log.events, "乙");
    let injected: Vec<&str> = yi_events.iter().map(|e| e.target.as_str()).collect();
    assert!(injected.is_empty(), "乙视角不该看到甲的注入／变更，实得：{injected:?}");
    let jia = kuyuan_view::events(&w, &log.events, "甲");
    assert_eq!(jia.len(), 2, "甲视角：注入 ＋ 变量变更");
    assert_eq!(jia[0].kind, "输入注入");
}

/// 逐条声明**覆盖**模块默认（§3 的 `存活` 那例）：私模块里也能有真档地址。
#[test]
fn per_address_visibility_overrides_module_default() {
    let t = format!(
        r#"{HEAD}"模块":[{{"id":"p1.甲","可见性":"私","归属":"甲","输入":[],"输出":[],"变量":[{{"名":"明","类型":"整数","初值":0,"可见性":"真"}},{{"名":"暗","类型":"整数","初值":0}}],"公式":[{{"名":"明.下一","目标":"明","表达式":["加",["引用","p1.甲.明"],["字面",1]]}},{{"名":"暗.下一","目标":"暗","表达式":["加",["引用","p1.甲.暗"],["字面",1]]}}]}}],"关系":[]}}"#
    );
    let (w, _) = run(&t, 1);
    let yi = kuyuan_view::snapshot(&w, "乙");
    assert!(yi.get("p1.甲.明").is_some(), "逐条标「真」应压过模块默认");
    assert!(yi.get("p1.甲.暗").is_none(), "未逐条标的仍随模块默认");
}

/// **投影不改世界**：同包同输入两次跑，全见结果与视角投影都不受影响（确定性 D6 ＋ 投影是纯函数）。
#[test]
fn views_do_not_change_the_world() {
    let t = tiered_pack();
    let (w1, e1) = run(&t, 5);
    let _ = kuyuan_view::snapshot(&w1, "甲");
    let _ = kuyuan_view::events(&w1, &e1, "甲");
    let (w2, e2) = run(&t, 5);
    assert_eq!(w1.snapshot_json(), w2.snapshot_json(), "投影过的世界应与没投影的一致");
    let a: Vec<String> = e1.iter().map(|e| serde_json::to_string(e).unwrap()).collect();
    let b: Vec<String> = e2.iter().map(|e| serde_json::to_string(e).unwrap()).collect();
    assert_eq!(a, b, "全量事件流不受投影影响");
    assert_eq!(
        kuyuan_view::snapshot_json(&w1, "甲"),
        kuyuan_view::snapshot_json(&w2, "甲"),
        "投影本身确定"
    );
}

/// 拒绝事件的口径：`目标` **不在场**（包名／未解析）＝公开——加载期事实，不是状态。
#[test]
fn reject_event_with_non_address_target_is_public() {
    let (w, _) = run(&tiered_pack(), 1);
    let mut evs = vec![kuyuan_log::Event {
        seq: 0,
        tick: 0,
        kind: kuyuan_log::KIND_REJECT.to_string(),
        target: "p1".to_string(),
        old: serde_json::Value::Null,
        new: serde_json::json!("三问缺项（拒绝加载）"),
        source: Some(kuyuan_log::SOURCE_LOAD.to_string()),
    }];
    // 混一条私有地址的拒绝：它对「乙」应消失
    evs.push(kuyuan_log::Event {
        seq: 1,
        tick: 1,
        kind: kuyuan_log::KIND_REJECT.to_string(),
        target: "p1.甲.v".to_string(),
        old: serde_json::Value::Null,
        new: serde_json::json!("不是输入（变量由公式写）"),
        source: Some(kuyuan_log::SOURCE_RUN.to_string()),
    });
    let yi = kuyuan_view::events(&w, &evs, "乙");
    assert_eq!(yi.len(), 1, "包名那条该留、私有地址那条该滤");
    assert_eq!(yi[0].target, "p1");
    assert_eq!(yi[0].seq, 0);
    assert_eq!(yi[0].source.as_deref(), Some("加载"), "来源字段随投影带出（裁 #31）");
    assert_eq!(kuyuan_view::events(&w, &evs, JUDGE).len(), 2);
}

// ---------- 反例：该拒的必须拒（§12 表 16–18） ----------

/// 16：档名不认识 → 拒绝加载（逐条 ＋ 模块默认两处都要拦）。
#[test]
fn unknown_visibility_is_rejected() {
    let per_addr = format!(
        r#"{HEAD}"模块":[{}{}],"关系":[]}}"#,
        r#"{"id":"p1.甲","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0,"可见性":"公开"}],"公式":[]}"#,
        ""
    );
    let e = kv::load_pack(&per_addr).err().expect("该拒");
    assert!(e.starts_with("未知可见性: p1.甲.v"), "实得：{e}");

    let per_mod = format!(
        r#"{HEAD}"模块":[{}],"关系":[]}}"#,
        r#"{"id":"p1.甲","可见性":"公开","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0}],"公式":[]}"#
    );
    let e = kv::load_pack(&per_mod).err().expect("该拒");
    assert!(e.starts_with("未知可见性: 模块 p1.甲"), "实得：{e}");
}

/// 17：私档而模块无归属 → 拒绝加载（不猜"大概是谁的"）。
#[test]
fn private_without_owner_is_rejected() {
    let t = format!(
        r#"{HEAD}"模块":[{}],"关系":[]}}"#,
        r#"{"id":"p1.甲","可见性":"私","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0}],"公式":[]}"#
    );
    let e = kv::load_pack(&t).err().expect("该拒");
    assert!(e.starts_with("私 档缺归属: p1.甲.v"), "实得：{e}");
    assert!(e.contains("未声明「归属」"));
}

/// 18：归属占用保留名 `裁判` → 拒绝加载。
#[test]
fn owner_cannot_take_reserved_name() {
    let t = format!(
        r#"{HEAD}"模块":[{}],"关系":[]}}"#,
        r#"{"id":"p1.甲","可见性":"私","归属":"裁判","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0}],"公式":[]}"#
    );
    let e = kv::load_pack(&t).err().expect("该拒");
    assert!(e.starts_with("归属占用保留名: 模块 p1.甲"), "实得：{e}");
}

/// 16 先于 17：档名不认识时**只报第一处**（两条缺陷同包，报 16 不报 17）。
#[test]
fn first_defect_only_in_table_order() {
    let t = format!(
        r#"{HEAD}"模块":[{}],"关系":[]}}"#,
        r#"{"id":"p1.甲","可见性":"公开","归属":"甲","输入":[],"输出":[],"变量":[{"名":"v","类型":"整数","初值":0,"可见性":"私"}],"公式":[]}"#
    );
    let e = kv::load_pack(&t).err().expect("该拒");
    assert!(e.starts_with("未知可见性: 模块 p1.甲"), "表序里 16 在 17 前，实得：{e}");
}

// ---------- 真包：duel32 双盲观战 ----------

const DUEL: &str = "32×32/duel32.json";
const SCRIPT: &str = "32×32/duel32.对局-01.json";

/// 双盲观战：玩家1 视角**看不到玩家2 的全部地址**，玩家1 自己的全部可见；裁判全见。
#[test]
fn duel32_player_views_are_double_blind() {
    let (w, _) = run(&kv::read_pack(DUEL), 1);
    let total = w.addr_of.len();
    let jia = kuyuan_view::snapshot(&w, "玩家1");
    let hidden: Vec<&String> = w
        .addr_of
        .iter()
        .filter(|a| a.contains(".玩家2"))
        .filter(|a| jia.get(a.as_str()).is_none())
        .collect();
    let p2_addrs = w.addr_of.iter().filter(|a| a.contains(".玩家2")).count();
    assert_eq!(hidden.len(), p2_addrs, "玩家2 的地址应一条不漏地被藏，实得藏 {}／{}", hidden.len(), p2_addrs);
    // 信息面批：己方地址中「敌方管道值」16 条应转裁判档——本方不可见、裁判可见
    let hid_own: Vec<&String> = w
        .addr_of
        .iter()
        .filter(|a| a.contains(".玩家1"))
        .filter(|a| jia.get(a.as_str()).is_none())
        .collect();
    let mut expect: Vec<String> = Vec::new();
    for i in 1..=2 {
        for n in ["敌x", "敌y", "爆启", "爆信", "爆x", "爆y"] {
            expect.push(format!("duel32.玩家1探{i}.{n}"));
        }
    }
    for n in ["爆2启", "爆2信", "爆2x", "爆2y"] {
        expect.push(format!("duel32.玩家1.{n}"));
    }
    let mut got: Vec<String> = hid_own.iter().map(|s| s.to_string()).collect();
    got.sort();
    expect.sort();
    assert_eq!(got, expect, "己方被藏的应恰为敌方管道值 16 条（信息面批）");
    assert!(
        w.addr_of.iter().filter(|a| a.contains(".玩家1")).count() - hid_own.len() > 60,
        "己方其余地址应照旧可见"
    );
    assert!(jia.get("duel32.场景.回合").is_some(), "场景是公开的");
    assert_eq!(kuyuan_view::visible_count(&w, "玩家1"), 109, "109／246（藏对手 121 ＋ 裁判档管道 16）");
    assert_eq!(total, 246);
    assert_eq!(kuyuan_view::visible_count(&w, JUDGE), total);
    assert!(
        kuyuan_view::snapshot(&w, JUDGE).get("duel32.玩家1探1.敌x").is_some(),
        "裁判照旧全见管道值"
    );
    assert!(
        jia.get("duel32.玩家1.爆向dx").is_some() && jia.get("duel32.玩家1.反炮1").is_some(),
        "爆炸方向／反炮等合法渠道仍可见"
    );
}

/// 真对局（12 拍 · `对局-01`）在视角下：裁判 329 条；玩家1 视角 134 条、玩家2 视角 112 条（信息面批后）——
/// **对手地址的事件一条不漏地被滤掉**（S7 验收：双盲观战）。
#[test]
fn duel32_real_match_under_views() {
    let pack: kuyuan_core::Pack = serde_json::from_str(&kv::read_pack(DUEL)).expect("包应可解析");
    let script: kuyuan_core::InputScript =
        serde_json::from_str(&kv::read_pack(SCRIPT)).expect("输入集应可解析");
    let mut w = kuyuan_driver::load(&pack).expect("应加载");
    let mut log = kuyuan_log::EventLog::new();
    for t in 1..=12u64 {
        let mut frame: std::collections::BTreeMap<String, kv::Value> =
            std::collections::BTreeMap::new();
        for f in script.frames.iter().filter(|f| f.tick == t) {
            for (k, v) in &f.writes {
                frame.insert(k.clone(), kuyuan_core::value_of_json(v).expect("值应可解析"));
            }
        }
        w.tick_in(t, &frame, &mut log).expect("应跑通");
    }

    assert_eq!(log.events.len(), 329, "全量＝裁判视角（与 S5′ 记录一致）");
    let jia = kuyuan_view::events(&w, &log.events, "玩家1");
    let yi = kuyuan_view::events(&w, &log.events, "玩家2");
    assert_eq!(jia.len(), 134, "玩家1 视角：134／329");
    assert_eq!(yi.len(), 112, "玩家2 视角：112／329");
    assert!(
        !jia.iter().any(|e| e.target.contains(".玩家2")),
        "玩家1 视角里不该有对手地址的事件"
    );
    assert!(!yi.iter().any(|e| e.target.contains(".玩家1")), "反向同理");
    let s = kuyuan_view::snapshot(&w, "玩家1");
    assert_eq!(s.get("duel32.玩家1.存活"), Some(&serde_json::json!(true)), "己方存活可见");
    assert!(s.get("duel32.玩家2.存活").is_none(), "对手存活不可见");
    assert!(s.get("duel32.场景.结果").is_some(), "场景公开");
}
