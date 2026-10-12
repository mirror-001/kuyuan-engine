//! S5′ · 随机对打不变式 —— 让引擎在成百局随机输入里自己撞边界（**过程检验**）。
//!
//! 随机源＝本文件的 LCG（不引外部依赖）；同种子同序列 ⇒ 可复跑。
//! 不变式：回合不超上限 · 结果在论域内 · 死了不复活 · 位置不越界 · 同输入同输出。

use kuyuan_verify as kv;

/// 线性同余发生器（确定性；只用于造输入）。
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// 0.0 .. 1.0
    fn unit(&mut self) -> f64 {
        (self.below(1_000_000) as f64) / 1_000_000.0
    }
}

/// 双方各四个入口：动作 · 方向dx · 方向dy · 射程。
const ADDRS: [[&str; 4]; 2] = [
    [
        "duel32.玩家1.动作",
        "duel32.玩家1.方向dx",
        "duel32.玩家1.方向dy",
        "duel32.玩家1.射程",
    ],
    [
        "duel32.玩家2.动作",
        "duel32.玩家2.方向dx",
        "duel32.玩家2.方向dy",
        "duel32.玩家2.射程",
    ],
];

/// 造一局（40 拍）的输入帧。
fn one_game(r: &mut Lcg) -> Vec<(u64, &'static str, kv::Value)> {
    let mut frames: Vec<(u64, &'static str, kv::Value)> = vec![];
    for t in 1..=40u64 {
        for a in ADDRS {
            let act = r.below(4); // 0 等待 · 1 移 · 2 探 · 3 爆
            let ang = r.unit() * std::f64::consts::TAU;
            let range = match act {
                2 => r.unit() * 6.0,
                3 => r.unit() * 16.0,
                _ => 0.0,
            };
            frames.push((t, a[0], kv::Value::Int(act as i64)));
            frames.push((t, a[1], kv::Value::Real(ang.cos())));
            frames.push((t, a[2], kv::Value::Real(ang.sin())));
            frames.push((t, a[3], kv::Value::Real(range)));
        }
    }
    frames
}

/// 不变式：300 局随机对打，一条都不许破。
#[test]
fn random_duels_keep_invariants() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut r = Lcg(20261004);
    let (mut 已结束, mut 平局, mut 有胜负, mut 最大事件) = (0u64, 0u64, 0u64, 0usize);
    for game in 0..300u64 {
        let frames = one_game(&mut r);
        let run = kv::run_pack_inputs(&text, 40, &frames)
            .unwrap_or_else(|e| panic!("第 {game} 局跑不通: {e}"));

        // ① 回合不超上限
        let turn = kv::get(&run.snapshot, "duel32.场景.回合").as_u64().unwrap_or(999);
        assert!(turn <= 40, "第 {game} 局：回合 {turn} 超上限");

        // ② 结果在论域内（0 未决 · 1 玩家1胜 · 2 玩家2胜 · 3 平）
        let res = kv::get(&run.snapshot, "duel32.场景.结果").as_u64().unwrap_or(999);
        assert!(res <= 3, "第 {game} 局：结果 {res} 越界");

        // ③ 死了不复活
        for who in 1..=2 {
            let addr = format!("duel32.玩家{who}.存活");
            let mut died = false;
            for e in run.events.iter().filter(|e| e.target == addr) {
                if e.new == serde_json::json!(false) {
                    died = true;
                } else if died && e.new == serde_json::json!(true) {
                    panic!("第 {game} 局：玩家{who} 复活了");
                }
            }
        }

        // ④ 位置不越界
        for who in 1..=2 {
            for axis in ["x", "y"] {
                let addr = format!("duel32.玩家{who}.{axis}");
                let v = kv::get(&run.snapshot, &addr).as_f64().unwrap_or(-1.0);
                assert!((0.0..=32.0).contains(&v), "第 {game} 局：{addr} = {v} 越界");
            }
        }

        // 覆盖度统计：若一局都分不出胜负，说明随机输入没打到要害 —— 检验是空转
        if res != 0 {
            已结束 += 1;
            if res == 3 {
                平局 += 1;
            } else {
                有胜负 += 1;
            }
        }
        最大事件 = 最大事件.max(run.events.len());
    }
    println!(
        "随机对打 300 局：已结束 {已结束}（平局 {平局} · 分出胜负 {有胜负}）· 单局最大事件数 {最大事件}"
    );
    assert!(有胜负 > 0, "300 局随机对打一局都没分出胜负——随机输入是空转，检验无效");
}

/// 同输入 ⇒ 同输出（D6 在真输入下成立）。
#[test]
fn random_duel_is_reproducible() {
    let text = kv::read_pack("32×32/duel32.json");
    let mut r = Lcg(7);
    let frames = one_game(&mut r);
    let a = kv::run_pack_inputs(&text, 40, &frames).expect("应跑通");
    let b = kv::run_pack_inputs(&text, 40, &frames).expect("应跑通");
    assert_eq!(kv::events_json(&a), kv::events_json(&b), "同输入应同输出");
}
