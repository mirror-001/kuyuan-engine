//! 选手库：`r`＝随机（量具）· `h`＝听风（标准）· `hx`＝猎手（进阶）· `h0`＝关耳朵对照。
//! 全都**只**读该侧视野快照——公平性由接口保证，不靠自觉。
//!
//! 猎手＝听声摸黑（循警／循声巡逻）＋ 情报射杀（扫描命中就精确掷弹）；
//! 带一组参数（内置档与壳内"自定义"共用）：
//! `hear`＝用不用耳朵（听炸）· `chase`＝追击窗口（拍）· `bomb_min／bomb_max`＝掷弹距离窗口。
//! 二段（2026-10-09 · 陪练进化）：外推（按扫描位移预测）· 分区蛇形扫荡 · 悬手（冷却中机动）
//! ——参数 `predict`／`sweep_mode`／`hover`；旧行为对照档 `h1`。

use crate::{Action, Policy, Rng, SIDE1, vb, vf, vi};

/// 8 向（与壳同款；随机策略用）。
const OCT: [(f64, f64); 8] = [
    (0.0, -1.0),
    (0.0, 1.0),
    (-1.0, 0.0),
    (1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
];

/// 猎手参数（内置档与"自定义"共用）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HunterParams {
    pub hear: bool,     // 用不用"耳朵"（听炸）
    pub chase: u64,     // 触点追击窗口（拍）
    pub bomb_min: f64,  // 掷弹自保下限（距己）
    pub bomb_age: u64,   // 掷弹窗口（拍）：扫描 ≤ 此拍龄才值得炸
    pub bomb_max: f64,  // 掷弹上限（≤16）
    // ---- 二段（陪练进化）：外推 / 分区扫荡 / 悬手 ----
    pub predict: u64,    // 外推前瞻（拍）：按扫描位移预测敌点（0=关）
    pub sweep_mode: u64, // 巡逻表：0=四角对角（旧）· 1=分区蛇形（新）
    pub hover: bool,     // 冷却中悬手机动（反侦察；false=原地等）
}

impl Default for HunterParams {
    fn default() -> Self {
        HunterParams { hear: true, chase: 12, bomb_min: 4.05, bomb_max: 16.0, bomb_age: 3, predict: 1, sweep_mode: 1, hover: true }
    }
}

/// 内置档位 → 参数；`None`＝不认识。
pub fn mode_params(spec: &str) -> Option<HunterParams> {
    match spec {
        "h" => Some(HunterParams::default()),
        "hx" => Some(HunterParams { chase: 18, bomb_age: 4, ..HunterParams::default() }),
        "h0" => Some(HunterParams { hear: false, ..HunterParams::default() }),
        // 旧行为对照（升级前）：不外交推 · 四角巡逻 · 站桩等冷却
        "h1" => Some(HunterParams { predict: 0, sweep_mode: 0, hover: false, ..HunterParams::default() }),
        _ => None,
    }
}

/// 造一个选手。`spec`: "r" 随机 · "h" 听风 · "hx" 猎手 · "h0" 关耳朵对照。
pub fn make(spec: &str, ns: &str, side: &str, seed: u64) -> Result<Box<dyn Policy>, String> {
    let prefix = format!("{ns}.{side}");
    match spec {
        "r" => Ok(random(side, seed)),
        "t" => Ok(dummy(ns, side, seed)),
        other => match mode_params(other) {
            Some(p) => Ok(Box::new(HeuristicPolicy::new(prefix, p))),
            None => Err(format!("选手不认识: {other}（可用 r / t / h / hx / h0 / h1）")),
        },
    }
}

/// 直接造一个随机（量具）（壳内「随机」档用；随机不读视野，故不带 ns）。
pub fn random(side: &str, seed: u64) -> Box<dyn Policy> {
    // 两侧随机流分开（同种子也不会镜像同步）。
    let s = if side == SIDE1 {
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    } else {
        seed.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
    };
    Box::new(RandomPolicy { rng: Rng::new(s ^ 0x52_4E_44) })
}
/// 直接造一个木桩（测试陪练）：原地不动 ＋ 每 3 拍掷一发（随机向 · 距离 8~16）。
pub fn dummy(ns: &str, side: &str, seed: u64) -> Box<dyn Policy> {
    // 两侧随机流分开（同种子也不会镜像同步）。
    let s = if side == SIDE1 {
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    } else {
        seed.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
    };
    Box::new(DummyPolicy { prefix: format!("{ns}.{side}"), rng: Rng::new(s ^ 0x74_4D_4D_59) })
}
/// 直接造一个猎手（壳内对手系统用：参数来自模式或自定义）。
pub fn hunter(ns: &str, side: &str, p: HunterParams) -> Box<dyn Policy> {
    Box::new(HeuristicPolicy::new(format!("{ns}.{side}"), p))
}

/// `r`：随机——10% 等 · 40% 移 · 25% 探 · 25% 爆（4~16）。**量具**；壳内「随机」档直接上桌（测试陪练）。
pub struct RandomPolicy {
    rng: Rng,
}

impl Policy for RandomPolicy {
    fn decide(&mut self, _tick: u64, _view: &serde_json::Value) -> Action {
        let r = self.rng.below(100);
        let d = self.rng.below(8) as usize;
        let (dx, dy) = OCT[d];
        match r {
            0..=9 => Action::wait(),
            10..=49 => Action::mv(dx, dy),
            50..=74 => Action::probe(dx, dy, self.rng.below(6) as f64 + 1.0),
            _ => Action::bomb(dx, dy, self.rng.below(13) as f64 + 4.0),
        }
    }
}

/// `t`：木桩（测试陪练）——原地不动；每 3 拍朝地图中心掷一发（距离 8~16）。
pub struct DummyPolicy {
    prefix: String,
    rng: Rng,
}
impl Policy for DummyPolicy {
    fn decide(&mut self, tick: u64, view: &serde_json::Value) -> Action {
        if tick % 3 == 0 {
            let x = vf(view, &format!("{}.x", self.prefix));
            let y = vf(view, &format!("{}.y", self.prefix));
            let gx = 16.0 - x;
            let gy = 16.0 - y;
            let d = (gx * gx + gy * gy).sqrt();
            let (ux, uy) = if d > 0.1 { (gx / d, gy / d) } else { (0.0, 1.0) };
            Action::bomb(ux, uy, self.rng.below(9) as f64 + 8.0)
        } else {
            Action::wait()
        }
    }
}

/// 情报触点（视野里"最近一次"看见的渠道之一）。
#[derive(Clone, Copy, Debug)]
struct Contact {
    x: f64,
    y: f64,
    tick: u64,
    kind: u8, // 0 扫描命中（敌方坐标）· 1 被警（敌探落点）· 2 反炮（爆炸点）· 3 听炸（可疑区）
}

/// 巡逻航点（无情报时按序走；探器也朝它打）——斜穿全图的大对角线，覆盖优先。
const SWEEP4: [(f64, f64); 4] =
    [(31.0, 31.0), (1.0, 1.0), (31.0, 1.0), (1.0, 31.0)];

/// 新表（sweep_mode=1）：分区蛇形（16 点 · 每 8×8 区中心）——覆盖均匀、不反复跑对角线。
fn sweep_point(mode: u64, i: usize) -> (f64, f64) {
    if mode == 0 {
        SWEEP4[i % SWEEP4.len()]
    } else {
        let k = i % 16;
        let iy = k / 4;
        let ix = if iy % 2 == 0 { k % 4 } else { 3 - (k % 4) };
        (ix as f64 * 8.0 + 4.0, iy as f64 * 8.0 + 4.0)
    }
}

/// 猎手：听声摸黑（循警／循声）＋ 情报射杀（扫描命中就精确掷弹）。
pub struct HeuristicPolicy {
    prefix: String,
    p: HunterParams,
    scan: Option<Contact>,
    scan_prev: Option<(f64, f64, u64)>, // 上次扫描（外推用）
    scan_vel: Option<(f64, f64)>,       // 扫描位移速度（格/拍）
    alert: Option<Contact>,
    counter: Option<Contact>,
    hear: Option<Contact>,
    sweep: usize,
}

impl HeuristicPolicy {
    fn new(prefix: String, p: HunterParams) -> HeuristicPolicy {
        HeuristicPolicy { prefix, p, scan: None, scan_prev: None, scan_vel: None, alert: None, counter: None, hear: None, sweep: 0 }
    }

    /// 供壳内对手系统读取当前参数（存档回显用）。
    pub fn params(&self) -> HunterParams {
        self.p
    }

    fn addr(&self, name: &str) -> String {
        format!("{}.{}", self.prefix, name)
    }

    fn probe_addr(&self, i: u64, name: &str) -> String {
        format!("{}探{}.{}", self.prefix, i, name)
    }

    /// 瞄准点：有扫描速度时按 predict 拍外推（clamp 到图内）。
    fn aim_at(&self, c: &Contact) -> (f64, f64) {
        if self.p.predict == 0 {
            return (c.x, c.y);
        }
        if let Some((vx, vy)) = self.scan_vel {
            let lead = self.p.predict as f64;
            return ((c.x + vx * lead).clamp(0.0, 32.0), (c.y + vy * lead).clamp(0.0, 32.0));
        }
        (c.x, c.y)
    }

    /// 最新触点（≤max_age 拍；同拍优先扫描，其次被警／反炮／听炸）。
    fn freshest(&self, tick: u64, max_age: u64) -> Option<(f64, f64)> {
        let prio = |c: &Contact| match c.kind {
            0 => 3u8,
            1 => 2,
            2 => 1,
            _ => 0,
        };
        [self.scan.as_ref(), self.alert.as_ref(), self.counter.as_ref(), self.hear.as_ref()]
            .into_iter()
            .flatten()
            .filter(|c| tick.saturating_sub(c.tick) <= max_age)
            .max_by_key(|c| (c.tick, prio(c)))
            .map(|c| (c.x, c.y))
    }

    fn waypoint(&self) -> (f64, f64) {
        sweep_point(self.p.sweep_mode, self.sweep)
    }

    /// 朝 goal 走一步（单位向量；贴边则刹掉越界分量；无路可走＝等待）。
    fn step(&self, me: (f64, f64), goal: (f64, f64)) -> Action {
        let (gx, gy) = (goal.0 - me.0, goal.1 - me.1);
        let d = (gx * gx + gy * gy).sqrt();
        if d < 0.05 {
            return Action::wait();
        }
        let mut sx = gx / d;
        let mut sy = gy / d;
        if me.0 + sx < 0.0 || me.0 + sx > 32.0 {
            sx = 0.0;
        }
        if me.1 + sy < 0.0 || me.1 + sy > 32.0 {
            sy = 0.0;
        }
        if sx == 0.0 && sy == 0.0 {
            return Action::wait();
        }
        Action::mv(sx, sy)
    }
}

impl Policy for HeuristicPolicy {
    fn decide(&mut self, tick: u64, view: &serde_json::Value) -> Action {
        // ---- 收情报：只从"该侧视野"里读（可见性已由引擎投影过） ----
        for i in 1..=2u64 {
            if vb(view, &format!("{}.找敌{}", self.prefix, i)) {
                let sx = vf(view, &self.probe_addr(i, "原x"))
                    + vf(view, &format!("{}.敌dx{}", self.prefix, i));
                let sy = vf(view, &self.probe_addr(i, "原y"))
                    + vf(view, &format!("{}.敌dy{}", self.prefix, i));
                if let Some((px, py, pt)) = self.scan_prev {
                    let dt = tick.saturating_sub(pt);
                    if dt >= 1 && dt <= 8 {
                        self.scan_vel = Some(((sx - px) / dt as f64, (sy - py) / dt as f64));
                    }
                }
                self.scan_prev = Some((sx, sy, tick));
                self.scan = Some(Contact {
                    x: vf(view, &self.probe_addr(i, "原x"))
                        + vf(view, &format!("{}.敌dx{}", self.prefix, i)),
                    y: vf(view, &self.probe_addr(i, "原y"))
                        + vf(view, &format!("{}.敌dy{}", self.prefix, i)),
                    tick,
                    kind: 0,
                });
            }
            if vb(view, &format!("{}.警b{}", self.prefix, i)) {
                self.alert = Some(Contact {
                    x: vf(view, &format!("{}.警x{}", self.prefix, i)),
                    y: vf(view, &format!("{}.警y{}", self.prefix, i)),
                    tick,
                    kind: 1,
                });
            }
            if vb(view, &format!("{}.反炮{}", self.prefix, i)) {
                self.counter = Some(Contact {
                    x: vf(view, &self.probe_addr(i, "原x"))
                        + vf(view, &format!("{}.反炮dx{}", self.prefix, i)),
                    y: vf(view, &self.probe_addr(i, "原y"))
                        + vf(view, &format!("{}.反炮dy{}", self.prefix, i)),
                    tick,
                    kind: 2,
                });
            }
            // 听炸（v0.3 默认包）：探器在飞时听到 16 内爆炸方位（只方向）——
            // 从"探器当前点"沿听向取 8 格作"可疑区"（爆炸点/敌人都可能在那一带）。
            if self.p.hear && vb(view, &self.probe_addr(i, "听到")) {
                let ox = vf(view, &self.probe_addr(i, "原x"));
                let oy = vf(view, &self.probe_addr(i, "原y"));
                let dx = vf(view, &self.probe_addr(i, "方向dx"));
                let dy = vf(view, &self.probe_addr(i, "方向dy"));
                let flown = vf(view, &self.probe_addr(i, "已飞"))
                    .min(vf(view, &self.probe_addr(i, "射程")));
                let px = ox + dx * flown;
                let py = oy + dy * flown;
                let hx = vf(view, &self.probe_addr(i, "听dx"));
                let hy = vf(view, &self.probe_addr(i, "听dy"));
                self.hear = Some(Contact {
                    x: (px + hx * 8.0).clamp(0.0, 32.0),
                    y: (py + hy * 8.0).clamp(0.0, 32.0),
                    tick,
                    kind: 3,
                });
            }
        }

        let me = (vf(view, &self.addr("x")), vf(view, &self.addr("y")));
        if !vb(view, &self.addr("存活")) {
            return Action::wait();
        }
        // 到点换下一个巡点（沿图巡逻）。
        if dist(me, self.waypoint()) <= 0.5 {
            self.sweep += 1;
        }

        let cool = vi(view, &self.addr("冷却"));
        let sel = (vi(view, &self.addr("探轮")) & 1) as u64;
        let flying = vb(view, &self.probe_addr(sel + 1, "启用"));
        if std::env::var_os("KUYUAN_ARENA_DEBUG").is_some() {
            if let Some(c) = self.scan {
                let d = dist(me, (c.x, c.y));
                eprintln!(
                    "[h] t{tick} cool{cool} age{} d={:.2} me=({:.1},{:.1}) scan=({:.1},{:.1})",
                    tick.saturating_sub(c.tick),
                    d,
                    me.0,
                    me.1,
                    c.x,
                    c.y
                );
            }
        }

        // ① 新鲜扫描（≤3 拍）——别坏"杀机会"：能炸就炸 · 太近先退 · 太远挪近 · 冷却则等。
        if let Some(c) = self.scan {
            let age = tick.saturating_sub(c.tick);
            if age <= self.p.bomb_age {
                let (ax, ay) = self.aim_at(&c);
                let d = dist(me, (ax, ay));
                if cool == 0 && d > self.p.bomb_min && d <= self.p.bomb_max {
                    return Action::bomb((ax - me.0) / d, (ay - me.1) / d, d);
                }
                if d <= self.p.bomb_min {
                    // 太近（炸了会同归于尽）——先退开，下一拍再炸。
                    let out = (me.0 + (me.0 - ax), me.1 + (me.1 - ay));
                    return self.step(me, out);
                }
                if d > self.p.bomb_max {
                    // 太远——朝它挪近（别开火，保住冷却）。
                    return self.step(me, (ax, ay));
                }
                // bomb_min < d ≤ bomb_max 且冷却中：
                if self.p.hover {
                    // 悬手（反侦察）：沿切向走一步——保持距离、别站桩。
                    let (ux, uy) = ((ax - me.0) / d, (ay - me.1) / d);
                    let s = if tick % 2 == 0 { 1.0 } else { -1.0 };
                    let (px, py) = (me.0 - uy * s, me.1 + ux * s);
                    if (0.0..=32.0).contains(&px) && (0.0..=32.0).contains(&py) {
                        return Action::mv(-uy * s, ux * s);
                    }
                    return Action::mv(uy * s, -ux * s); // 贴边则反向
                }
                return Action::wait();
            }
        }

        // ② 无新鲜扫描——探一发（选中的探器没在飞时）。
        let aim = self.freshest(tick, self.p.chase).unwrap_or_else(|| self.waypoint());
        if cool == 0 && !flying {
            let d = dist(me, aim);
            if d >= 0.4 {
                return Action::probe((aim.0 - me.0) / d, (aim.1 - me.1) / d, d.min(6.0));
            }
        }

        // ③ 移动：朝最新情报点；没情报就巡逻。
        let goal = self.freshest(tick, self.p.chase).unwrap_or_else(|| self.waypoint());
        if dist(me, goal) > 0.5 {
            return self.step(me, goal);
        }
        Action::wait()
    }
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小视图：me、冷却、可选"扫描命中"（敌坐标）。
    fn mkview(me: (f64, f64), cool: i64, scan: Option<(f64, f64)>) -> serde_json::Value {
        let mut v = serde_json::json!({
            "X.x": me.0, "X.y": me.1,
            "X.存活": true, "X.冷却": cool, "X.探轮": 0,
            "X探1.启用": false, "X探1.射程": 6.0, "X探1.已飞": 0.0,
            "X探1.原x": 0.0, "X探1.原y": 0.0, "X探1.方向dx": 0.0, "X探1.方向dy": 0.0
        });
        if let Some((sx, sy)) = scan {
            v["X.找敌1"] = serde_json::json!(true);
            v["X.敌dx1"] = serde_json::json!(sx);
            v["X.敌dy1"] = serde_json::json!(sy);
        }
        v
    }

    /// 二段 · 外推：第二次扫描（速度 +2/拍）后应瞄准外推点，而不是旧点。
    #[test]
    fn h_predict_leads_target() {
        let p = HunterParams { hear: false, ..HunterParams::default() };
        let mut h = HeuristicPolicy::new("X".into(), p);
        let a1 = h.decide(1, &mkview((0.0, 25.0), 0, Some((10.0, 10.0))));
        assert_eq!(a1.act, 1, "太远应挪近: {a1:?}");
        let a2 = h.decide(2, &mkview((0.0, 25.0), 0, Some((12.0, 10.0))));
        assert_eq!(a2.act, 1, "仍远应挪近: {a2:?}");
        let (dx, dy) = a2.dir;
        let n = (dx * dx + dy * dy).sqrt();
        let ux = dx / n;
        assert!(ux > 0.66, "应按外推瞄准（ux≈0.68；未外推≈0.625），实得 ux={ux:.3}");
    }

    /// 二段 · 分区蛇形：在(4,4)（第 0 航点）→ 换点后应朝东(12,4)，而不是旧表的大对角(1,1)。
    #[test]
    fn h_sweep_serpentine() {
        let p = HunterParams { hear: false, ..HunterParams::default() };
        let mut h = HeuristicPolicy::new("X".into(), p);
        let a = h.decide(1, &mkview((4.0, 4.0), 0, None));
        assert_eq!(a.act, 2, "闲时应打探: {a:?}");
        let (dx, dy) = a.dir;
        assert!(dx > 0.95 && dy.abs() < 0.05, "应朝东（12,4），实得 dir=({dx:.2},{dy:.2})");
    }

    /// 二段 · 悬手：冷却中（d∈窗）应切向机动，而不是站桩。
    #[test]
    fn h_hover_when_cooling() {
        let p = HunterParams { hear: false, ..HunterParams::default() };
        let mut h = HeuristicPolicy::new("X".into(), p);
        let a = h.decide(1, &mkview((0.0, 10.0), 1, Some((10.0, 10.0))));
        assert_eq!(a.act, 1, "冷却中应悬手机动（非站桩）: {a:?}");
        let (dx, dy) = a.dir;
        assert!(dx.abs() < 0.05 && (dy.abs() - 1.0).abs() < 0.05, "应沿切向（±y），实得 dir=({dx:.2},{dy:.2})");
    }
}