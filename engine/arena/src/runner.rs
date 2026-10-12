//! 竞技场 · 对局器（runner）：进程内跑一局——出生（t1）→ 每拍「读视野 → 出招 → 写入口 → 步进」，
//! 记写面（验收"只写入口"）、判死因、收统计。

use std::collections::BTreeMap;

use kuyuan_core::{Pack, Value};
use kuyuan_driver::World;

use crate::{Action, Policy, SIDE1, SIDE2};

/// 一局的结果（＋死因 ＋双方统计）。
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub seed: u64,
    pub result: i64, // 1 P1胜 · 2 P2胜 · 3 平
    pub ticks: u64,  // 终局拍
    pub cause: Cause,
    pub spawn: [(f64, f64); 2], // 双方出生点（t1）——复核分布用
    pub p1: SideStats,
    pub p2: SideStats,
}

/// 对局报告：结果 ＋ 写面（每条写：拍、键）。
#[derive(Debug, Clone)]
pub struct MatchReport {
    pub outcome: Outcome,
    pub writes: Vec<(u64, String)>,
}

/// 终局方式（死因归类；"其他"＝没归上，值得查）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    P1Bomb,
    P2Bomb,
    P1Self,
    P2Self,
    Both,
    Clock,
    Other,
}

impl Cause {
    pub fn label(self) -> &'static str {
        match self {
            Cause::P1Bomb => "P1炸杀",
            Cause::P2Bomb => "P2炸杀",
            Cause::P1Self => "P1自炸",
            Cause::P2Self => "P2自炸",
            Cause::Both => "双死",
            Cause::Clock => "40拍平",
            Cause::Other => "其他",
        }
    }
    pub fn idx(self) -> usize {
        match self {
            Cause::P1Bomb => 0,
            Cause::P2Bomb => 1,
            Cause::P1Self => 2,
            Cause::P2Self => 3,
            Cause::Both => 4,
            Cause::Clock => 5,
            Cause::Other => 6,
        }
    }
    pub const ALL: [Cause; 7] = [
        Cause::P1Bomb,
        Cause::P2Bomb,
        Cause::P1Self,
        Cause::P2Self,
        Cause::Both,
        Cause::Clock,
        Cause::Other,
    ];
}

/// 一侧的逐局统计。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SideStats {
    pub moves: u64,
    pub probes: u64,
    pub bombs: u64,
    pub scans: u64,
    pub warns: u64,
    pub counters: u64,
    pub hears: u64,
}

/// 跑一局：`max_ticks`＝安全拍上限（含出生拍；正常 40 回合在 ≈t41 收）。
pub fn run_match(
    pack_text: &str,
    seed: u64,
    p1_spec: &str,
    p2_spec: &str,
    max_ticks: u64,
) -> Result<MatchReport, String> {
    let pack: Pack = serde_json::from_str(pack_text).map_err(|e| format!("解析包失败: {e}"))?;
    let ns = pack.ns.clone();
    let mut log = kuyuan_log::EventLog::new();
    let mut world =
        kuyuan_driver::load_logged(&pack, &mut log).map_err(|e| format!("加载被拒: {e}"))?;
    let mut p1: Box<dyn Policy> = crate::policies::make(p1_spec, &ns, SIDE1, seed)?;
    let mut p2: Box<dyn Policy> = crate::policies::make(p2_spec, &ns, SIDE2, seed)?;

    // ---- t1 出生（与壳同款配方：∈[0,32]² · 一位小数 · 相距 ≥1） ----
    let mut wrng = crate::Rng::new(seed ^ 0xD1CE_5EED);
    let a = wrng.pos01();
    let b = loop {
        let p = wrng.pos01();
        let (dx, dy) = (p.0 - a.0, p.1 - a.1);
        if dx * dx + dy * dy >= 1.0 {
            break p;
        }
    };
    let mut writes: Vec<(u64, String)> = Vec::new();
    let mut f0: BTreeMap<String, Value> = BTreeMap::new();
    f0.insert(format!("{ns}.{SIDE1}.出生x"), Value::Real(a.0));
    f0.insert(format!("{ns}.{SIDE1}.出生y"), Value::Real(a.1));
    f0.insert(format!("{ns}.{SIDE2}.出生x"), Value::Real(b.0));
    f0.insert(format!("{ns}.{SIDE2}.出生y"), Value::Real(b.1));
    for k in f0.keys() {
        writes.push((1, k.clone()));
    }
    world.tick_in(1, &f0, &mut log).map_err(|e| format!("tick1 失败: {e}"))?;

    let mut stats = [SideStats::default(), SideStats::default()];
    let mut prev = Snap::of(&world, &ns);
    let mut pf = Flags::of(&world, &ns);
    let mut killed: Option<((bool, bool), Snap)> = None;
    let cap = max_ticks.max(2);

    while world.tick < cap {
        if scene_result(&world, &ns) != 0 {
            break;
        }
        let t = world.tick + 1;
        let v1 = kuyuan_view::snapshot(&world, SIDE1);
        let v2 = kuyuan_view::snapshot(&world, SIDE2);
        let a1 = p1.decide(t, &v1);
        let a2 = p2.decide(t, &v2);
        tally(&mut stats[0], a1);
        tally(&mut stats[1], a2);

        let mut frame: BTreeMap<String, Value> = BTreeMap::new();
        put_action(&mut frame, &ns, SIDE1, a1);
        put_action(&mut frame, &ns, SIDE2, a2);
        for k in frame.keys() {
            writes.push((t, k.clone()));
        }

        let n0 = log.events.len();
        world
            .tick_in(t, &frame, &mut log)
            .map_err(|e| format!("tick {t} 失败: {e}"))?;
        for e in &log.events[n0..] {
            if e.kind == kuyuan_log::KIND_REJECT {
                return Err(format!(
                    "第 {t} 拍出现拒绝（选手写面不该出这种事）: {} → {}",
                    e.target, e.new
                ));
            }
        }

        let now = Snap::of(&world, &ns);
        let nf = Flags::of(&world, &ns);
        pulses(&mut stats, &nf, &pf);
        let k1 = prev.alive(1) && !now.alive(1);
        let k2 = prev.alive(2) && !now.alive(2);
        if (k1 || k2) && killed.is_none() {
            killed = Some(((k1, k2), prev.clone()));
        }
        pf = nf;
        prev = now;
    }

    let result = scene_result(&world, &ns);
    let cause = classify(result, &killed);
    Ok(MatchReport {
        outcome: Outcome {
            seed,
            result,
            ticks: world.tick,
            cause,
            spawn: [a, b],
            p1: stats[0].clone(),
            p2: stats[1].clone(),
        },
        writes,
    })
}

// ---------- 小件 ----------

fn scene_result(w: &World, ns: &str) -> i64 {
    match w.get(&format!("{ns}.场景.结果")) {
        Some(Value::Int(i)) => *i,
        _ => 0,
    }
}

fn tally(s: &mut SideStats, a: Action) {
    match a.act {
        1 => s.moves += 1,
        2 => s.probes += 1,
        3 => s.bombs += 1,
        _ => {}
    }
}

fn put_action(frame: &mut BTreeMap<String, Value>, ns: &str, side: &str, a: Action) {
    frame.insert(format!("{ns}.{side}.动作"), Value::Int(a.act));
    frame.insert(format!("{ns}.{side}.方向dx"), Value::Real(a.dir.0));
    frame.insert(format!("{ns}.{side}.方向dy"), Value::Real(a.dir.1));
    frame.insert(format!("{ns}.{side}.射程"), Value::Real(a.range));
}

/// 关键状态的“拍前快照”（死因判定与翻转检测用）。
#[derive(Clone)]
struct Snap {
    liv: [bool; 2],
    pos: [(f64, f64); 2],
    bomb: [(bool, bool, f64, f64); 2], // (启用, 引信, 落点x, 落点y)
}

impl Snap {
    fn of(w: &World, ns: &str) -> Snap {
        let mut liv = [false; 2];
        let mut pos = [(0.0, 0.0); 2];
        let mut bomb = [(false, false, 0.0, 0.0); 2];
        for k in 1..=2usize {
            let p = format!("{ns}.玩家{k}");
            liv[k - 1] = wb(w, &format!("{p}.存活"));
            pos[k - 1] = (wf(w, &format!("{p}.x")), wf(w, &format!("{p}.y")));
            let b = format!("{p}爆");
            bomb[k - 1] = (
                wb(w, &format!("{b}.启用")),
                wb(w, &format!("{b}.引信")),
                wf(w, &format!("{b}.落点x")),
                wf(w, &format!("{b}.落点y")),
            );
        }
        Snap { liv, pos, bomb }
    }

    fn alive(&self, who: usize) -> bool {
        self.liv[who - 1]
    }
}

/// 感知渠道的“本拍脉冲”标志（用于统计发生次数）。
#[derive(Clone, PartialEq)]
struct Flags {
    s: [[bool; 8]; 2], // 找敌1 · 找敌2 · 反炮1 · 反炮2 · 警b1 · 警b2 · 听到1 · 听到2
}

impl Flags {
    fn of(w: &World, ns: &str) -> Flags {
        let mut s = [[false; 8]; 2];
        for k in 1..=2usize {
            let p = format!("{ns}.玩家{k}");
            for i in 0..2usize {
                s[k - 1][i] = wb(w, &format!("{p}.找敌{}", i + 1));
                s[k - 1][2 + i] = wb(w, &format!("{p}.反炮{}", i + 1));
                s[k - 1][4 + i] = wb(w, &format!("{p}.警b{}", i + 1));
                s[k - 1][6 + i] = wb(w, &format!("{p}.听到{}", i + 1));
            }
        }
        Flags { s }
    }
}

fn pulses(stats: &mut [SideStats; 2], now: &Flags, prev: &Flags) {
    for k in 0..2usize {
        for j in 0..8usize {
            if !prev.s[k][j] && now.s[k][j] {
                match j {
                    0 | 1 => stats[k].scans += 1,
                    2 | 3 => stats[k].counters += 1,
                    4 | 5 => stats[k].warns += 1,
                    _ => stats[k].hears += 1,
                }
            }
        }
    }
}

/// 死因候选：某拍“在炸”的弹里，落点距死者 ≤ 爆径 4 的（返回弹主 1/2）。
fn killers(s: &Snap, who: usize) -> Vec<u8> {
    let p = s.pos[who - 1];
    let mut out = Vec::new();
    for b in 0..2usize {
        let (en, fu, bx, by) = s.bomb[b];
        if en && fu {
            let (dx, dy) = (bx - p.0, by - p.1);
            if dx * dx + dy * dy <= 16.0 + 1e-9 {
                out.push((b + 1) as u8);
            }
        }
    }
    out
}

fn classify(result: i64, killed: &Option<((bool, bool), Snap)>) -> Cause {
    match killed {
        None => {
            if result == 3 {
                Cause::Clock
            } else {
                Cause::Other
            }
        }
        Some(((true, true), _)) => Cause::Both,
        Some(((true, false), s)) => {
            let c = killers(s, 1);
            if c.contains(&2) {
                Cause::P2Bomb
            } else if c.contains(&1) {
                Cause::P1Self
            } else {
                Cause::Other
            }
        }
        Some(((false, true), s)) => {
            let c = killers(s, 2);
            if c.contains(&1) {
                Cause::P1Bomb
            } else if c.contains(&2) {
                Cause::P2Self
            } else {
                Cause::Other
            }
        }
        Some(((false, false), _)) => Cause::Other,
    }
}

fn wf(w: &World, a: &str) -> f64 {
    w.get(a).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn wb(w: &World, a: &str) -> bool {
    matches!(w.get(a), Some(Value::Bool(true)))
}