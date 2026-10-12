//! 竞技场自测：种子复跑 · 写面只许入口 · 批跑冒烟。

use kuyuan_arena::{read_pack, run_match};

fn pack() -> String {
    read_pack("32×32/duel32.json")
}

/// 同种子 ⇒ 同对局（选手随机性与出生均从种子导出）。
#[test]
fn same_seed_same_outcome() {
    let t = pack();
    let a = run_match(&t, 42, "r", "r", 48).expect("应跑通");
    let b = run_match(&t, 42, "r", "r", 48).expect("应跑通");
    assert_eq!(format!("{:?}", a.outcome), format!("{:?}", b.outcome), "同种子应同对局");
}

/// 写面只许入口：每拍每侧恰 4 值（动作/方向dx/方向dy/射程），t1 只许出生 2 值。
#[test]
fn writes_are_limited_to_input_door() {
    let t = pack();
    let r = run_match(&t, 7, "h", "r", 48).expect("应跑通");
    let mut per_tick: std::collections::BTreeMap<(u64, String), usize> = Default::default();
    for (tick, key) in &r.writes {
        let side = if key.starts_with("duel32.玩家1.") {
            "玩家1"
        } else if key.starts_with("duel32.玩家2.") {
            "玩家2"
        } else {
            panic!("越侧写: {key}");
        };
        let suffix_ok = key.ends_with(".动作")
            || key.ends_with(".方向dx")
            || key.ends_with(".方向dy")
            || key.ends_with(".射程");
        let birth_ok = *tick == 1 && (key.ends_with(".出生x") || key.ends_with(".出生y"));
        assert!(suffix_ok || birth_ok, "越门写: t{tick} {key}");
        *per_tick.entry((*tick, side.to_string())).or_insert(0) += 1;
    }
    assert!(!per_tick.is_empty(), "一局下来一条写都没有——夹具没接上");
    for ((tick, side), cnt) in &per_tick {
        let expect = if *tick == 1 { 2 } else { 4 };
        assert_eq!(*cnt, expect, "t{tick} {side} 写数应 {expect}，实得 {cnt}");
    }
}

/// 冒烟：R vs R 批跑——结果域内 · 有分出胜负 · 探器确实在飞。
#[test]
fn r_vs_r_smoke() {
    let t = pack();
    let (mut decided, mut draws, mut probes) = (0u64, 0u64, 0u64);
    for seed in 100..140u64 {
        let r = run_match(&t, seed, "r", "r", 48).expect("应跑通");
        assert!((1..=3).contains(&r.outcome.result), "结果越界: {}", r.outcome.result);
        if r.outcome.result == 3 {
            draws += 1;
        } else {
            decided += 1;
        }
        probes += r.outcome.p1.probes + r.outcome.p2.probes;
    }
    assert!(decided > 0, "40 局随机对打一局都没分出胜负——批跑空转");
    assert!(probes > 0, "40 局随机对打一发探器都没发——选手没接上");
    println!("R vs R 40 局：分出胜负 {decided} · 平局 {draws} · 发探合计 {probes}");
}

/// 出生铺得开：两侧的 x／y 都要覆盖全图（防"邻种子相关"把某侧钉在半边图）。
#[test]
fn spawns_are_spread_for_both_sides() {
    let t = pack();
    let n = 200u64;
    let mut sum = [[0.0f64; 2]; 2];
    let mut mn = [[f64::MAX; 2]; 2];
    let mut mx = [[f64::MIN; 2]; 2];
    for seed in 1..=n {
        let r = run_match(&t, seed, "r", "r", 8).expect("应跑通");
        for side in 0..2 {
            for axis in 0..2 {
                let p = r.outcome.spawn[side];
                let v = if axis == 0 { p.0 } else { p.1 };
                sum[side][axis] += v;
                mn[side][axis] = mn[side][axis].min(v);
                mx[side][axis] = mx[side][axis].max(v);
            }
        }
    }
    for side in 0..2 {
        for axis in 0..2 {
            let mean = sum[side][axis] / n as f64;
            assert!(
                mn[side][axis] < 10.0 && mx[side][axis] > 22.0,
                "侧{side} 轴{axis} 出生没铺开：min {} max {}",
                mn[side][axis],
                mx[side][axis]
            );
            assert!((12.0..=20.0).contains(&mean), "侧{side} 轴{axis} 出生均值偏：{mean}");
        }
    }
}

/// H 对 R：固定种子段上，H 应稳定优于 R（"会玩"的底线证人）。
#[test]
fn h_beats_r_over_fixed_seeds() {
    let t = pack();
    let (mut hw, mut rw, mut scans) = (0u64, 0u64, 0u64);
    for seed in 1..=200u64 {
        let r = run_match(&t, seed, "h", "r", 48).expect("应跑通");
        match r.outcome.result {
            1 => hw += 1,
            2 => rw += 1,
            _ => {}
        }
        scans += r.outcome.p1.scans;
    }
    assert!(scans > 0, "H 一局扫描都没出——听声摸黑没接上");
    assert!(hw > rw, "H 应优于 R：H {hw} vs R {rw}");
    println!("H vs R 200 局：H {hw} · R {rw} · H 扫描 {scans}");
}