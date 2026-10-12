//! 库元引擎 · arena —— 竞技场（进程内自动对局器；32×32 平衡件批）
//!
//! 一句话：**让"选手"在引擎里自己打**——每拍读**该侧玩家视野**（`view` 投影，与真人同一份），
//! 出 4 值（动作 · 方向 · 射程），由竞技场写成输入帧喂给世界。**同种子 ⇒ 同对局**。
//!
//! 铁律（结构保证，不靠自觉）：
//! - 选手**只**拿到该侧视野快照——别的什么都拿不到（接口里没有别的）；
//! - 选手**只**能返回 `Action`——世界的写面收口在竞技场：入口 4 值（＋ t1 出生）；
//! - 出生与选手随机性都从种子导出 ⇒ 可复跑、可对账。

pub mod policies;
pub mod runner;

pub use runner::{run_match, Cause, MatchReport, Outcome, SideStats};

/// 包读取（`engine/arena` 上两级＝工作区父目录）。
pub fn read_pack(rel: &str) -> String {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读包失败 {}: {e}", p.display()))
}

pub const SIDE1: &str = "玩家1";
pub const SIDE2: &str = "玩家2";

/// 一位选手本拍的动作（唯一出口——写面由竞技场收口）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Action {
    pub act: i64,        // 0 等待 · 1 移动 · 2 探 · 3 爆
    pub dir: (f64, f64), // 方向（等待＝(0,0)）
    pub range: f64,      // 射程（探 ≤6 · 爆 ≤16）
}

impl Action {
    pub fn wait() -> Action {
        Action { act: 0, dir: (0.0, 0.0), range: 0.0 }
    }
    pub fn mv(dx: f64, dy: f64) -> Action {
        Action { act: 1, dir: (dx, dy), range: 0.0 }
    }
    pub fn probe(dx: f64, dy: f64, r: f64) -> Action {
        Action { act: 2, dir: (dx, dy), range: r }
    }
    pub fn bomb(dx: f64, dy: f64, r: f64) -> Action {
        Action { act: 3, dir: (dx, dy), range: r }
    }
}

/// 选手接口：**只**拿到当时该侧玩家视野（`kuyuan_view::snapshot(&world, side)` 的产物）。
pub trait Policy {
    fn decide(&mut self, tick: u64, view: &serde_json::Value) -> Action;
}

static NULL: serde_json::Value = serde_json::Value::Null;

/// 视野取值（查不到＝Null）。
pub fn vget<'a>(view: &'a serde_json::Value, addr: &str) -> &'a serde_json::Value {
    view.get(addr).unwrap_or(&NULL)
}

pub fn vf(view: &serde_json::Value, addr: &str) -> f64 {
    vget(view, addr).as_f64().unwrap_or(0.0)
}

pub fn vb(view: &serde_json::Value, addr: &str) -> bool {
    matches!(vget(view, addr), serde_json::Value::Bool(true))
}

pub fn vi(view: &serde_json::Value, addr: &str) -> i64 {
    let v = vget(view, addr);
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).unwrap_or(0)
}

/// 种子混匀（splitmix64 收尾）——防"只差低位的一串种子"抽出一串相关随机。
pub fn mix_seed(z: u64) -> u64 {
    let mut z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 确定性随机源（xorshift64；入种先混匀——同种子同流，邻种子不相关）。
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(mix_seed(seed) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// 随机一点：∈[0,32]²，取一位小数（与壳同款配方）。
    pub fn pos01(&mut self) -> (f64, f64) {
        (
            (self.unit() * 320.0).round() / 10.0,
            (self.unit() * 320.0).round() / 10.0,
        )
    }
}