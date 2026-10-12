//! 网页对局壳 · 会话（门槛件批②）
//!
//! 一核：载包 → 开局 → 每拍〔你 4 值 ＋ AI 4 值〕同帧 → 结算 → 出状态。
//! 薄适配：本版只识 duel32（玩家1＝你 · 玩家2＝对手（听风／猎手／随机／自定义））；出生／存读档随后接。

use std::collections::BTreeMap;

use kuyuan_core::{json_of_value, InputFrame, Value};
use kuyuan_arena::Policy;
use kuyuan_arena::policies::{dummy, hunter, random, HunterParams};

pub const HUMAN: &str = "玩家1";
pub const AI: &str = "玩家2";

/// 壳存档标准（套 store 的「存档」；另加壳自有 rng）。
pub const PLAY_SAVE_STD: &str = "库元/play/存档/v0";

/// 爆炸方向（感知渠道）：一声爆炸 · (拍 · dx · dy · 自弹? · 自弹落点x/y · 本体当拍x/y)。
#[derive(Clone, Debug, PartialEq)]
pub struct Blast {
    pub tick: u64,
    pub dx: f64,
    pub dy: f64,
    pub mine: bool,
    pub mx: f64,
    pub my: f64,
    pub px: f64,
    pub py: f64,
}

/// 8 向单位向量（1↑ 2↓ 3← 4→ 5↖ 6↗ 7↙ 8↘）。
const DIRS: [(f64, f64); 8] = [
    (0.0, -1.0),
    (0.0, 1.0),
    (-1.0, 0.0),
    (1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
];

/// 对手（人机）：模式 ＋ 生效参数（自定义时即所见即所得）。
#[derive(Clone, Debug, PartialEq)]
pub struct Opponent {
    pub mode: String, // "standard"（听风）｜"hunter"（猎手）｜"random"（随机）｜"dummy"（木桩）｜"custom"（自定义）
    pub hear: bool,
    pub chase: u64,
    pub bomb_min: f64,
    pub bomb_max: f64,
    pub bomb_age: u64,
}

impl Default for Opponent {
    /// 默认＝听风（标准）。
    fn default() -> Self {
        Opponent { mode: "standard".into(), hear: true, chase: 12, bomb_min: 4.05, bomb_max: 16.0, bomb_age: 3 }
    }
}

impl Opponent {
    /// 内置档位。
    pub fn preset(mode: &str) -> Option<Opponent> {
        match mode {
            "standard" => Some(Opponent::default()),
            "hunter" => Some(Opponent { mode: "hunter".into(), chase: 18, bomb_age: 4, ..Opponent::default() }),
            "random" => Some(Opponent { mode: "random".into(), ..Opponent::default() }),
            "dummy" => Some(Opponent { mode: "dummy".into(), ..Opponent::default() }),
            "custom" => Some(Opponent { mode: "custom".into(), ..Opponent::default() }),
            _ => None,
        }
    }

    /// 从请求体解析（缺省项用档位默认；越界拒绝）。
    pub fn request(v: &serde_json::Value) -> Result<Opponent, String> {
        let mode = v.get("mode").and_then(|x| x.as_str()).unwrap_or("standard");
        let base = Opponent::preset(mode)
            .ok_or_else(|| format!("模式不认识: {mode}（可用 standard / hunter / random / dummy / custom）"))?;
        let spec = Opponent {
            mode: mode.to_string(),
            hear: v.get("hear").and_then(|x| x.as_bool()).unwrap_or(base.hear),
            chase: v.get("chase").and_then(|x| x.as_u64()).unwrap_or(base.chase),
            bomb_min: v.get("bomb_min").and_then(|x| x.as_f64()).unwrap_or(base.bomb_min),
            bomb_max: v.get("bomb_max").and_then(|x| x.as_f64()).unwrap_or(base.bomb_max),
            bomb_age: v.get("bomb_age").and_then(|x| x.as_u64()).unwrap_or(base.bomb_age),
        };
        spec.validate()?;
        Ok(spec)
    }

    /// 从存档解析（旧档无此段＝default）。
    pub fn from_json(v: &serde_json::Value) -> Result<Opponent, String> {
        let mode = v.get("mode").and_then(|x| x.as_str()).unwrap_or("standard");
        let base = Opponent::preset(mode)
            .ok_or_else(|| format!("存档对手模式不认识: {mode}"))?;
        let spec = Opponent {
            mode: mode.to_string(),
            hear: v.get("hear").and_then(|x| x.as_bool()).unwrap_or(base.hear),
            chase: v.get("chase").and_then(|x| x.as_u64()).unwrap_or(base.chase),
            bomb_min: v.get("bomb_min").and_then(|x| x.as_f64()).unwrap_or(base.bomb_min),
            bomb_max: v.get("bomb_max").and_then(|x| x.as_f64()).unwrap_or(base.bomb_max),
            bomb_age: v.get("bomb_age").and_then(|x| x.as_u64()).unwrap_or(base.bomb_age),
        };
        spec.validate()?;
        Ok(spec)
    }

    /// 校验（拒优于猜）。
    pub fn validate(&self) -> Result<(), String> {
        if !(4..=24).contains(&self.chase) {
            return Err(format!("追击窗口要在 4–24 拍（实得 {}）", self.chase));
        }
        if !(4.0..=8.0).contains(&self.bomb_min) {
            return Err(format!("掷弹最近要在 4–8（实得 {}）", self.bomb_min));
        }
        if !(8.0..=16.0).contains(&self.bomb_max) {
            return Err(format!("掷弹最远要在 8–16（实得 {}）", self.bomb_max));
        }
        if self.bomb_min >= self.bomb_max {
            return Err(format!("掷弹最近 {} 应小于最远 {}", self.bomb_min, self.bomb_max));
        }
        if !(1..=5).contains(&self.bomb_age) {
            return Err(format!("掷弹窗口要在 1–5 拍（实得 {}）", self.bomb_age));
        }
        Ok(())
    }

    /// 造策略（对手＝玩家2：随机／猎手系；seed＝随机档的流种子，供存读档重建）。
    pub fn build(&self, ns: &str, seed: u64) -> Result<Box<dyn Policy>, String> {
        self.validate()?;
        if self.mode == "random" {
            return Ok(random(AI, seed));
        }
        if self.mode == "dummy" {
            return Ok(dummy(ns, AI, seed));
        }
        Ok(hunter(
            ns,
            AI,
            HunterParams {
                hear: self.hear,
                chase: self.chase,
                bomb_min: self.bomb_min,
                bomb_max: self.bomb_max,
                bomb_age: self.bomb_age,
                ..HunterParams::default() // 二段（外推/扫荡/悬手）：默认即升级后行为
            },
        ))
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "mode": self.mode, "hear": self.hear, "chase": self.chase,
            "bomb_min": self.bomb_min, "bomb_max": self.bomb_max, "bomb_age": self.bomb_age,
        })
    }

    pub fn name(&self) -> String {
        match self.mode.as_str() {
            "hunter" => "猎手".to_string(),
            "random" => "随机".to_string(),
            "dummy" => "木桩".to_string(),
            "custom" => "自定义".to_string(),
            _ => "听风".to_string(),
        }
    }
}

pub struct Session {
    pack_text: String,
    ns: String,
    world: kuyuan_driver::World,
    log: kuyuan_log::EventLog,
    /// 帧存档（为存读档／复跑备着）。
    frames: Vec<kuyuan_core::InputFrame>,
    rng: u64,
    /// 目击记忆（三渠道）：(拍, x, y)。
    warns: Vec<(u64, f64, f64)>,
    /// 报告：(拍, 相对dx, 相对dy, 绝对x, 绝对y)。
    reports: Vec<(u64, f64, f64, f64, f64)>,
    /// 反炮：(拍, x, y)。
    fires: Vec<(u64, f64, f64)>,
    /// 听炸（实验变体）：(拍, 探号, 方位dx, 方位dy)。
    hears: Vec<(u64, u64, f64, f64)>,
    /// 爆炸方向（感知渠道）：Blast（含自弹判据等）。
    blasts: Vec<Blast>,
    /// 对手（人机）：配置 ＋ 策略（只吃对手视野）。
    opp_spec: Opponent,
    opp: Box<dyn Policy>,
}

impl Session {
        pub fn new(pack_text: &str, seed: u64) -> Result<Session, String> {
        Session::with_opponent(pack_text, seed, Opponent::default())
    }

    /// 带对手构局（人机模式／自定义）。
    pub fn with_opponent(pack_text: &str, seed: u64, spec: Opponent) -> Result<Session, String> {
        let pack: kuyuan_core::Pack =
            serde_json::from_str(pack_text).map_err(|e| format!("解析包失败: {e}"))?;
        let mut log = kuyuan_log::EventLog::new();
        let world = kuyuan_driver::load_logged(&pack, &mut log).map_err(|e| format!("加载被拒: {e}"))?;
        let rng = kuyuan_arena::mix_seed(seed) | 1; // 与壳 rng 同源：随机档的流种子（存读档可重建）
        let opp = spec.build(&pack.ns, rng)?;
        let mut ss = Session {
            pack_text: pack_text.to_string(),
            ns: pack.ns.clone(),
            world,
            log,
            frames: Vec::new(),
            rng: kuyuan_arena::mix_seed(seed) | 1, // 种子先混匀（防"启动时刻的纳秒种子"抽结构化出生）
            warns: Vec::new(),
            reports: Vec::new(),
            fires: Vec::new(),
            hears: Vec::new(),
            blasts: Vec::new(),
            opp_spec: spec,
            opp,
        };
        ss.open()?;
        Ok(ss)
    }

    pub fn restart(&mut self, seed: u64) -> Result<(), String> {
                let spec = self.opp_spec.clone();
        *self = Session::with_opponent(&self.pack_text, seed, spec)?;
        Ok(())
    }

    /// 换对手（新局前调用；只换人，不动世界）。
    pub fn set_opponent(&mut self, spec: Opponent) -> Result<(), String> {
        self.opp = spec.build(&self.ns, self.rng)?;
        self.opp_spec = spec;
        Ok(())
    }

    /// 开局随机落位（t1＝落位拍：只注入出生，双方不出手；坐标实录进帧）。
    fn open(&mut self) -> Result<(), String> {
        // 落位随机数＝开局一次性事务（帧已实录）：不进入壳 rng 账本——壳 rng 专供“续跑种子”语义。
        let keep = self.rng;
        let a = self.rnd_pos();
        let b = loop {
            let p = self.rnd_pos();
            let (dx, dy) = (p.0 - a.0, p.1 - a.1);
            if dx * dx + dy * dy >= 1.0 {
                break p;
            }
        };
        self.rng = keep; // 账本复位：随机档对手的随机流要能随存读档重建
        let t = self.world.tick + 1;
        let mut writes: BTreeMap<String, Value> = BTreeMap::new();
        writes.insert(self.ps(HUMAN, "出生x"), Value::Real(a.0));
        writes.insert(self.ps(HUMAN, "出生y"), Value::Real(a.1));
        writes.insert(self.ps(AI, "出生x"), Value::Real(b.0));
        writes.insert(self.ps(AI, "出生y"), Value::Real(b.1));
        self.world.tick_in(t, &writes, &mut self.log)?;
        self.record(t, &writes);
        Ok(())
    }

    /// 随机一点：∈[0,32]²，取一位小数。
    fn rnd_pos(&mut self) -> (f64, f64) {
        let x = (self.rnd01() * 320.0).round() / 10.0;
        let y = (self.rnd01() * 320.0).round() / 10.0;
        (x, y)
    }

    fn rnd01(&mut self) -> f64 {
        (self.rnd() >> 11) as f64 / (1u64 << 53) as f64
    }

    // ---------- 地址与取值 ----------

    fn ps(&self, side: &str, name: &str) -> String {
        format!("{}.{}.{}", self.ns, side, name)
    }

    fn pmod(&self, side: &str, i: u32) -> String {
        format!("{}.{}探{}", self.ns, side, i)
    }

    fn bmod(&self, side: &str) -> String {
        format!("{}.{}爆", self.ns, side)
    }

    fn vf(&self, addr: &str) -> f64 {
        self.world.get(addr).and_then(|v| v.as_f64()).unwrap_or(0.0)
    }

    fn vb(&self, addr: &str) -> bool {
        matches!(self.world.get(addr), Some(Value::Bool(true)))
    }

    fn vi(&self, addr: &str) -> i64 {
        match self.world.get(addr) {
            Some(Value::Int(i)) => *i,
            _ => 0,
        }
    }

    pub fn result(&self) -> i64 {
        self.vi(&self.ps("场景", "结果"))
    }

    // ---------- 一拍 ----------

    /// 一拍的完整交互：校验你的输入 → 生成 AI 输入 → 同帧结算。
    pub fn act(&mut self, act: i64, dir: Option<i64>, range: Option<f64>) -> Result<(), String> {
        if self.result() != 0 {
            return Err("对局已结束——点「新局」再来一盘".into());
        }
        if !(0..=3).contains(&act) {
            return Err(format!("动作不认识: {act}（0等待·1移动·2探·3爆）"));
        }
        let mut dirn = 0i64;
        let mut rng_val = 0.0f64;
        if act >= 1 {
            let d = dir.ok_or("要选方向（1–8）")?;
            if !(1..=8).contains(&d) {
                return Err(format!("方向不认识: {d}"));
            }
            dirn = d;
        }
        if act == 2 {
            let r = range.ok_or("探：要填射程（1–6）")?;
            if !(r > 0.0 && r <= 6.0) {
                return Err(format!("探的射程要在 1–6（实得 {r}）"));
            }
            rng_val = r;
        }
        if act == 3 {
            let r = range.ok_or("爆：要填射程（1–16）")?;
            if !(r > 0.0 && r <= 16.0) {
                return Err(format!("爆的射程要在 1–16（实得 {r}）"));
            }
            rng_val = r;
        }
        let (dx, dy) = if act >= 1 { DIRS[(dirn - 1) as usize] } else { (0.0, 0.0) };

        // 你
        let mut writes: BTreeMap<String, Value> = BTreeMap::new();
        writes.insert(self.ps(HUMAN, "动作"), Value::Int(act));
        writes.insert(self.ps(HUMAN, "方向dx"), Value::Real(dx));
        writes.insert(self.ps(HUMAN, "方向dy"), Value::Real(dy));
        writes.insert(self.ps(HUMAN, "射程"), Value::Real(rng_val));

                // 对手：只吃"自己视角"（界投影）——公平由视野收口在服务端。
                let a2 = self.opp.decide(self.world.tick + 1, &kuyuan_view::snapshot(&self.world, AI));
                //（方向由对手策略直接给出单位向量）
        writes.insert(self.ps(AI, "动作"), Value::Int(a2.act));
        writes.insert(self.ps(AI, "方向dx"), Value::Real(a2.dir.0));
        writes.insert(self.ps(AI, "方向dy"), Value::Real(a2.dir.1));
        writes.insert(self.ps(AI, "射程"), Value::Real(a2.range));

        let t = self.world.tick + 1;
        self.world.tick_in(t, &writes, &mut self.log)?;

        self.record(t, &writes);
        self.harvest();
        Ok(())
    }

    /// 记一帧（复盘／存读档用的实录）。
    fn record(&mut self, t: u64, writes: &BTreeMap<String, Value>) {
        let mut jw: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for (k, v) in writes {
            jw.insert(k.clone(), json_of_value(v));
        }
        self.frames.push(kuyuan_core::InputFrame { tick: t, writes: jw });
    }


    fn rnd(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// 把本拍的“五条感知渠道”捞进记忆（只读己方可见数值）。
    fn harvest(&mut self) {
        let m = collect_memory(&self.world, &self.ns, &self.log.events);
        self.warns.extend(m.warns);
        self.reports.extend(m.reports);
        self.fires.extend(m.fires);
        self.hears.extend(m.hears);
        self.blasts.extend(m.blasts);
    }

    // ---------- 终局复盘（全场视角） ----------

    /// 复盘：按帧重演到第 upto 拍，返回**裁判视角**的双方快照。仅终局后开放（防剧透）。
    pub fn replay_json(&self, upto: u64) -> Result<serde_json::Value, String> {
        if self.result() == 0 {
            return Err("对局未结束——复盘只在终局后开放".into());
        }
        let pack: kuyuan_core::Pack =
            serde_json::from_str(&self.pack_text).map_err(|e| format!("解析包失败: {e}"))?;
        let mut w = kuyuan_driver::load(&pack).map_err(|e| format!("加载被拒: {e}"))?;
        let mut log = kuyuan_log::EventLog::new();
        let last = self.frames.last().map(|f| f.tick).unwrap_or(0);
        let upto = upto.min(last);
        for f in &self.frames {
            if f.tick > upto {
                break;
            }
            apply_frame(&mut w, &mut log, f)?;
        }
        let mut p1 = side_snapshot(&w, &self.ns, HUMAN);
        let mut p2 = side_snapshot(&w, &self.ns, AI);
        // 爆炸方向（该拍新响 · 两侧）：复盘里也画
        for (side, snap) in [(HUMAN, &mut p1), (AI, &mut p2)] {
            let tgt = format!("{}.{}.爆向dx", self.ns, side);
            if log.events.iter().any(|e| e.tick == upto && e.target == tgt) {
                let dx = wf(&w, &format!("{}.{}.爆向dx", self.ns, side));
                let dy = wf(&w, &format!("{}.{}.爆向dy", self.ns, side));
                (*snap)["blast"] = serde_json::json!([dx, dy]);
            }
        }
        Ok(serde_json::json!({
            "tick": upto,
            "total": last,
            "result": wi(&w, &format!("{}.场景.结果", self.ns)),
            "p1": p1,
            "p2": p2,
        }))
    }

    // ---------- 存读档（直连 store：引用式＋重算式） ----------

    /// 存档：套 store 的「存档」（存档即验），另加壳自有 rng（续跑等价的前提）。
    pub fn save_json(&self) -> Result<serde_json::Value, String> {
        let arch = kuyuan_store::save(self.pack_text.as_bytes(), &self.frames, &self.world, &self.log)?;
        Ok(serde_json::json!({
            "标准": PLAY_SAVE_STD,
            "元": {"拍": self.world.tick},
            "存档": serde_json::to_value(&arch).map_err(|e| format!("存档序列化失败: {e}"))?,
                        "壳": {"rng": self.rng.to_string(), "对手": self.opp_spec.to_json()},
        }))
    }

    /// 读档：三道校验＋重演走 store（唯一一处重演逻辑）；壳侧记忆按同一输入集复跑重收。
    pub fn load(pack_text: &str, save: &serde_json::Value) -> Result<Session, String> {
        let stdv = save.get("标准").and_then(|v| v.as_str()).ok_or("存档缺「标准」")?;
        if stdv != PLAY_SAVE_STD {
            return Err(format!("壳存档标准不认识: {stdv}"));
        }
        let arch: kuyuan_store::Archive = serde_json::from_value(
            save.get("存档").cloned().ok_or("存档缺「存档」")?,
        )
        .map_err(|e| format!("「存档」段解析失败: {e}"))?;
        let rng = save
            .get("壳")
            .and_then(|c| c.get("rng"))
            .and_then(|v| v.as_str())
            .ok_or("存档缺「壳.rng」")?
            .parse::<u64>()
            .map_err(|e| format!("壳 rng 解析失败: {e}"))?;

        // 对手（缺省＝标准；旧档无此段）。
        let spec = match save.get("壳").and_then(|c| c.get("对手")) {
            Some(v) => Opponent::from_json(v)?,
            None => Opponent::default(),
        };

        let (world, log) = kuyuan_store::replay(pack_text.as_bytes(), &arch)?;

        // 壳侧记忆：轻量复跑（封条已由 store 验过；这里只为重收“目击记录”）
        let pack: kuyuan_core::Pack =
            serde_json::from_str(pack_text).map_err(|e| format!("包解析失败: {e}"))?;
        let mut w = kuyuan_driver::load(&pack).map_err(|e| format!("加载被拒: {e}"))?;
        let mut l = kuyuan_log::EventLog::new();
        let mut mem = Memory::default();
        let mut opp = spec.build(&pack.ns, rng)?;
        let last_tick = arch.frames.last().map(|f| f.tick).unwrap_or(0);
        for f in &arch.frames {
            apply_frame(&mut w, &mut l, f)?;
                        mem.absorb(collect_memory(&w, &pack.ns, &l.events));
            if f.tick < last_tick {
                let v2 = kuyuan_view::snapshot(&w, AI);
                let _ = opp.decide(f.tick + 1, &v2);
            }
        }
        if kuyuan_store::seal_of(&w, &l) != arch.seal {
            return Err("壳侧复跑与存档封条不符 —— 拒绝读档".into());
        }

        Ok(Session {
            pack_text: pack_text.to_string(),
            ns: pack.ns.clone(),
            world,
            log,
            frames: arch.frames.clone(),
            rng,
            warns: mem.warns,
            reports: mem.reports,
            fires: mem.fires,
            hears: mem.hears,
            blasts: mem.blasts,
            opp_spec: spec,
            opp,
        })
    }

    /// 当前拍（存档响应里回给页面）。
    pub fn tick(&self) -> u64 {
        self.world.tick
    }

    // ---------- 状态 ----------

    pub fn state_json(&self) -> serde_json::Value {
        let g = |n: &str| self.ps(HUMAN, n);

        let mut probes = Vec::new();
        for i in 1..=2 {
            let m = self.pmod(HUMAN, i);
            let from = [self.vf(&format!("{m}.原x")), self.vf(&format!("{m}.原y"))];
            let d = [self.vf(&format!("{m}.方向dx")), self.vf(&format!("{m}.方向dy"))];
            let range = self.vf(&format!("{m}.射程"));
            let flown = self.vf(&format!("{m}.已飞"));
            let gone = flown.min(range);
            let heard = if self.vb(&format!("{m}.听到")) {
                serde_json::json!([self.vf(&format!("{m}.听dx")), self.vf(&format!("{m}.听dy"))])
            } else {
                serde_json::Value::Null
            };
            probes.push(serde_json::json!({
                "i": i,
                "active": self.vb(&format!("{m}.启用")),
                "from": from,
                "to": [from[0] + d[0] * range, from[1] + d[1] * range],
                "cur": [from[0] + d[0] * gone, from[1] + d[1] * gone],
                "range": range,
                "flown": flown,
                "heard": heard,
            }));
        }

        let m = self.bmod(HUMAN);
        let bombs = vec![serde_json::json!({
            "i": 1,
            "armed": self.vb(&format!("{m}.启用")),
            "fuse": self.vb(&format!("{m}.引信")),
            "x": self.vf(&format!("{m}.落点x")),
            "y": self.vf(&format!("{m}.落点y")),
        })];

        let warns: Vec<_> = self
            .warns
            .iter()
            .map(|(t, x, y)| serde_json::json!({"tick": t, "x": x, "y": y}))
            .collect();
        let reports: Vec<_> = self
            .reports
            .iter()
            .map(|(t, dx, dy, x, y)| serde_json::json!({"tick": t, "dx": dx, "dy": dy, "x": x, "y": y}))
            .collect();
        let fires: Vec<_> = self
            .fires
            .iter()
            .map(|(t, x, y)| serde_json::json!({"tick": t, "x": x, "y": y}))
            .collect();
        let hears: Vec<_> = self
            .hears
            .iter()
            .map(|(t, i, dx, dy)| serde_json::json!({"tick": t, "i": i, "dx": dx, "dy": dy}))
            .collect();
        let blasts: Vec<_> = self
            .blasts
            .iter()
            .map(|b| serde_json::json!({"tick": b.tick, "dx": b.dx, "dy": b.dy, "mine": b.mine, "mx": b.mx, "my": b.my, "px": b.px, "py": b.py}))
            .collect();
        let blast = match self.blasts.last() {
            Some(b) if b.tick == self.world.tick => serde_json::json!([b.dx, b.dy]),
            _ => serde_json::Value::Null,
        };

        serde_json::json!({
            "tick": self.world.tick,
            "next": self.world.tick + 1,
            "result": self.result(),
            "alive": self.vb(&g("存活")),
            "pos": [self.vf(&g("x")), self.vf(&g("y"))],
            "cooldown": self.vi(&g("冷却")),
            "probe_round": self.vi(&g("探轮")),
            "probes": probes,
            "bombs": bombs,
            "warns": warns,
            "reports": reports,
            "fires": fires,
            "hears": hears,
            "blasts": blasts,
            "blast": blast,
            "frames": self.frames.len(),
            "opp": self.opp_spec.to_json(),
            "opp_name": self.opp_spec.name(),
            "log": self.log_lines(),
        })
    }

    fn log_lines(&self) -> Vec<String> {
        let evs = kuyuan_view::events(&self.world, &self.log.events, HUMAN);
        let sel: Vec<&kuyuan_log::Event> = evs.iter().filter(|e| e.kind != "搬运").collect();
        let start = sel.len().saturating_sub(80);
        sel[start..].iter().map(|e| fmt_event(&self.ns, e)).collect()
    }
}

/// 壳侧记忆（感知渠道的“目击记录”）。
#[derive(Default)]
struct Memory {
    warns: Vec<(u64, f64, f64)>,
    reports: Vec<(u64, f64, f64, f64, f64)>,
    fires: Vec<(u64, f64, f64)>,
    hears: Vec<(u64, u64, f64, f64)>,
    blasts: Vec<Blast>,
}

impl Memory {
    fn absorb(&mut self, o: Memory) {
        self.warns.extend(o.warns);
        self.reports.extend(o.reports);
        self.fires.extend(o.fires);
        self.hears.extend(o.hears);
        self.blasts.extend(o.blasts);
    }
}

/// 读一遍“本拍”的四条感知渠道（正常对局与读档重建共用此一处）。
fn collect_memory(world: &kuyuan_driver::World, ns: &str, evs: &[kuyuan_log::Event]) -> Memory {
    let t = world.tick;
    let ps = |side: &str, name: &str| format!("{ns}.{side}.{name}");
    let pmod = |side: &str, i: u32| format!("{ns}.{side}探{i}");
    let vf = |a: &str| world.get(a).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let vb = |a: &str| matches!(world.get(a), Some(Value::Bool(true)));
    let mut m = Memory::default();
    for i in 1..=2 {
        // 报告：探 i 扫到敌（相对发射位；绝对＝发射位＋相对）
        if vb(&ps(HUMAN, &format!("找敌{i}"))) {
            let dx = vf(&ps(HUMAN, &format!("敌dx{i}")));
            let dy = vf(&ps(HUMAN, &format!("敌dy{i}")));
            let pm = pmod(HUMAN, i);
            let ox = vf(&format!("{pm}.原x"));
            let oy = vf(&format!("{pm}.原y"));
            m.reports.push((t, dx, dy, ox + dx, oy + dy));
        }
        // 警告：对手探 i 扫到我（给出敌探位置）
        if vb(&ps(HUMAN, &format!("警b{i}"))) {
            let x = vf(&ps(HUMAN, &format!("警x{i}")));
            let y = vf(&ps(HUMAN, &format!("警y{i}")));
            m.warns.push((t, x, y));
        }
        // 反炮：我探 i 与爆炸相交（给出爆炸精确位置）
        if vb(&ps(HUMAN, &format!("反炮{i}"))) {
            let dx = vf(&ps(HUMAN, &format!("反炮dx{i}")));
            let dy = vf(&ps(HUMAN, &format!("反炮dy{i}")));
            let pm = pmod(HUMAN, i);
            let ox = vf(&format!("{pm}.原x"));
            let oy = vf(&format!("{pm}.原y"));
            m.fires.push((t, ox + dx, oy + dy));
        }
        // 听炸（v0.3 转正 · 默认包）：我探 i 听到爆炸的大致方位（只有方向）
        let hm = pmod(HUMAN, i);
        if vb(&format!("{hm}.听到")) {
            let hdx = vf(&format!("{hm}.听dx"));
            let hdy = vf(&format!("{hm}.听dy"));
            m.hears.push((t, i as u64, hdx, hdy));
        }
    }
    // 爆炸方向（感知渠道：耳闻）：本拍「爆向dx」有变更事件＝一声新的爆炸
    let tgt = format!("{ns}.{HUMAN}.爆向dx");
    if evs.iter().any(|e| e.tick == t && e.kind == "变量变更" && e.target == tgt) {
        // 自弹判据（事件流）：本拍「引信 真→假」＝自己的弹炸响（结算后世界值已复位，不能读当前值）
        let mine = evs.iter().any(|e| e.tick == t && e.kind == "变量变更"
            && e.target == format!("{ns}.{HUMAN}爆.引信")
            && e.old.as_bool() == Some(true));
        m.blasts.push(Blast {
            tick: t,
            dx: vf(&ps(HUMAN, "爆向dx")),
            dy: vf(&ps(HUMAN, "爆向dy")),
            mine,
            mx: if mine { vf(&format!("{ns}.{HUMAN}爆.落点x")) } else { 0.0 },
            my: if mine { vf(&format!("{ns}.{HUMAN}爆.落点y")) } else { 0.0 },
            px: vf(&ps(HUMAN, "x")),
            py: vf(&ps(HUMAN, "y")),
        });
    }
    m
}

/// 应用一帧（读档重建用）：json 写值 → 世界一次 tick。
fn apply_frame(
    world: &mut kuyuan_driver::World,
    log: &mut kuyuan_log::EventLog,
    f: &InputFrame,
) -> Result<(), String> {
    let mut writes: BTreeMap<String, Value> = BTreeMap::new();
    for (k, v) in &f.writes {
        writes.insert(k.clone(), kuyuan_core::value_of_json(v)?);
    }
    world
        .tick_in(f.tick, &writes, log)
        .map_err(|e| format!("重演被拒（第 {} 拍）: {e}", f.tick))
}

fn fmt_event(ns: &str, e: &kuyuan_log::Event) -> String {
    let target = e.target.strip_prefix(&format!("{ns}.")).unwrap_or(&e.target);
    let tag = match e.kind.as_str() {
        "输入注入" => "注",
        "变量变更" => "变",
        "拒绝" => "拒",
        other => other,
    };
    format!("t{} ·{} {} {}→{}", e.tick, tag, target, disp(&e.old), disp(&e.new))
}

fn disp(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Bool(true) => "真".into(),
        serde_json::Value::Bool(false) => "假".into(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}


fn side_snapshot(w: &kuyuan_driver::World, ns: &str, side: &str) -> serde_json::Value {
    let addr = |n: &str| format!("{ns}.{side}.{n}");
    let pm = |i: u32| format!("{ns}.{side}探{i}");
    let bm = format!("{ns}.{side}爆");
    let mut probes = Vec::new();
    for i in 1..=2 {
        let m = pm(i);
        let from = [wf(w, &format!("{m}.原x")), wf(w, &format!("{m}.原y"))];
        let d = [wf(w, &format!("{m}.方向dx")), wf(w, &format!("{m}.方向dy"))];
        let range = wf(w, &format!("{m}.射程"));
        let flown = wf(w, &format!("{m}.已飞"));
        let gone = flown.min(range);
        let heard = if wb(w, &format!("{m}.听到")) {
            serde_json::json!([wf(w, &format!("{m}.听dx")), wf(w, &format!("{m}.听dy"))])
        } else {
            serde_json::Value::Null
        };
        probes.push(serde_json::json!({
            "i": i,
            "active": wb(w, &format!("{m}.启用")),
            "from": from,
            "to": [from[0] + d[0] * range, from[1] + d[1] * range],
            "cur": [from[0] + d[0] * gone, from[1] + d[1] * gone],
            "heard": heard,
        }));
    }
    let bombs = vec![serde_json::json!({
        "i": 1,
        "armed": wb(w, &format!("{bm}.启用")),
        "x": wf(w, &format!("{bm}.落点x")),
        "y": wf(w, &format!("{bm}.落点y")),
    })];
    serde_json::json!({
        "x": wf(w, &addr("x")),
        "y": wf(w, &addr("y")),
        "alive": wb(w, &addr("存活")),
        "probes": probes,
        "bombs": bombs,
    })
}

fn wf(w: &kuyuan_driver::World, a: &str) -> f64 {
    w.get(a).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn wb(w: &kuyuan_driver::World, a: &str) -> bool {
    matches!(w.get(a), Some(Value::Bool(true)))
}

fn wi(w: &kuyuan_driver::World, a: &str) -> i64 {
    match w.get(a) {
        Some(Value::Int(i)) => *i,
        _ => 0,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = include_str!("../../../packs/32×32/duel32.json");

    /// 朝中心走一步（永远界内；用于测试的确定性人类脚本）。
    fn toward_center(s: &Session) -> i64 {
        let x = s.vf(&s.ps(HUMAN, "x"));
        let y = s.vf(&s.ps(HUMAN, "y"));
        let sx: i64 = if 16.0 - x > 0.0 { 1 } else if 16.0 - x < 0.0 { -1 } else { 0 };
        let sy: i64 = if 16.0 - y > 0.0 { 1 } else if 16.0 - y < 0.0 { -1 } else { 0 };
        match (sx, sy) {
            (0, -1) => 1,
            (0, 1) => 2,
            (-1, 0) => 3,
            (1, 0) => 4,
            (-1, -1) => 5,
            (1, -1) => 6,
            (-1, 1) => 7,
            (1, 1) => 8,
            _ => 1,
        }
    }

    /// 跑一段脚本（动一步；对局若结束则停）。
    fn script_run(s: &mut Session, from: u64, to: u64) {
        for _ in from..=to {
            if s.result() != 0 {
                break;
            }
            let d = toward_center(s);
            if s.act(1, Some(d), None).is_err() {
                break;
            }
        }
    }

        /// 文件回环必须逐字节等价：帧里的"长小数"经 JSON 文本往返不得丢 1 ulp（serde_json `float_roundtrip`）。
    #[test]
        fn file_roundtrip_survives_save_load() {
                for seed in 1u64..=120 {
            let mut a = Session::new(PACK, seed.wrapping_mul(1_000_003)).expect("开局");
            let n = 4 + (seed % 3);
            for i in 0..n {
                let d: i64 = if i % 2 == 0 { 4 } else { 2 };
                if a.act(1, Some(d), None).is_err() {
                    break;
                }
            }
            let save = a.save_json().expect("应能存档");
                        // （省去内存回环：下面的文件回环已含两遍重建）
            let text = save.to_string();
            let back: serde_json::Value = serde_json::from_str(&text).expect("应是 JSON");
            let r = Session::load(PACK, &back);
            if let Err(e) = r {
                                println!("REPRO seed={seed} n={n} err={e}");
                let f1 = &save["存档"]["输入"];
                let f2 = &back["存档"]["输入"];
                println!("frames value equal: {}", f1 == f2);
                println!("frames text equal: {}", f1.to_string() == f2.to_string());
                if f1 != f2 {
                    let a1 = f1.as_array().unwrap();
                    let a2 = f2.as_array().unwrap();
                    for (i2, (x, y)) in a1.iter().zip(a2.iter()).enumerate() {
                        if x != y {
                            println!("frame {i2} differs:");
                            println!("  A={x}");
                            println!("  B={y}");
                        }
                    }
                }
                panic!("repro");
            }
        }
        println!("no repro in 1..=200");
    }

    /// 换对手：自定义参数下"存→读→续 ≡ 不中断"（对手记忆按帧复跑重建）。
    #[test]
    fn custom_opponent_save_load_is_byte_equal() {
        let spec = Opponent { mode: "custom".into(), hear: false, chase: 20, bomb_min: 4.5, bomb_max: 12.0, bomb_age: 4 };
        let mut a = Session::with_opponent(PACK, 20261009, spec.clone()).expect("开局");
        script_run(&mut a, 2, 10);
        let save = a.save_json().expect("应能存档");
        let t_save = a.world.tick;
        script_run(&mut a, t_save + 1, t_save + 6);
        let mut b = Session::load(PACK, &save).expect("应能读档");
        assert_eq!(b.opp_spec, spec, "对手配置应随档还原");
        script_run(&mut b, t_save + 1, t_save + 6);
        assert_eq!(
            kuyuan_store::seal_of(&b.world, &b.log),
            kuyuan_store::seal_of(&a.world, &a.log),
            "自定义对手下续跑封条应逐字节相等"
        );
    }

    /// 随机档：存→读→续 ≡ 不中断（随机流随“壳 rng”重建）。
    #[test]
    fn random_opponent_save_load_is_byte_equal() {
        let spec = Opponent::preset("random").expect("随机档应在册");
        let mut a = Session::with_opponent(PACK, 20261010, spec.clone()).expect("开局");
        script_run(&mut a, 2, 12);
        let save = a.save_json().expect("应能存档");
        let t_save = a.world.tick;
        script_run(&mut a, t_save + 1, t_save + 6);
        let mut b = Session::load(PACK, &save).expect("应能读档");
        assert_eq!(b.opp_spec, spec, "随机档配置应随档还原");
        script_run(&mut b, t_save + 1, t_save + 6);
        assert_eq!(
            kuyuan_store::seal_of(&b.world, &b.log),
            kuyuan_store::seal_of(&a.world, &a.log),
            "随机档下续跑封条应逐字节相等"
        );
    }

    /// 木桩档：存→读→续 ≡ 不中断（随机流随“壳 rng”重建）。
    #[test]
    fn dummy_opponent_save_load_is_byte_equal() {
        let spec = Opponent::preset("dummy").expect("木桩档应在册");
        let mut a = Session::with_opponent(PACK, 20261011, spec.clone()).expect("开局");
        script_run(&mut a, 2, 12);
        let save = a.save_json().expect("应能存档");
        let t_save = a.world.tick;
        script_run(&mut a, t_save + 1, t_save + 6);
        let mut b = Session::load(PACK, &save).expect("应能读档");
        assert_eq!(b.opp_spec, spec, "木桩档配置应随档还原");
        script_run(&mut b, t_save + 1, t_save + 6);
        assert_eq!(
            kuyuan_store::seal_of(&b.world, &b.log),
            kuyuan_store::seal_of(&a.world, &a.log),
            "木桩档下续跑封条应逐字节相等"
        );
    }

    /// 存→读→续 ≡ 不中断（封条逐字节相等；rng 还原后 AI 同序）。
    #[test]
    fn save_load_resume_is_byte_equal() {
        let mut a = Session::new(PACK, 20261008).expect("开局");
        script_run(&mut a, 2, 12);
        let save = a.save_json().expect("应能存档");
        let t_save = a.world.tick;
        let seal_save = kuyuan_store::seal_of(&a.world, &a.log);
        let mem = (a.warns.clone(), a.reports.clone(), a.fires.clone(), a.hears.clone());

        script_run(&mut a, t_save + 1, t_save + 8);

        let mut b = Session::load(PACK, &save).expect("应能读档");
        assert_eq!(b.world.tick, t_save, "读档应停在存档拍");
        assert_eq!(
            kuyuan_store::seal_of(&b.world, &b.log),
            seal_save,
            "重建应与存档点逐字节相等"
        );
        assert_eq!(b.warns, mem.0, "目击记忆（警）应重建");
        assert_eq!(b.reports, mem.1, "目击记忆（报告）应重建");
        assert_eq!(b.fires, mem.2, "目击记忆（反炮）应重建");
        assert_eq!(b.hears, mem.3, "目击记忆（听炸）应重建");

        script_run(&mut b, t_save + 1, t_save + 8);
        assert_eq!(b.world.tick, a.world.tick, "续跑拍数应一致");
        assert_eq!(
            kuyuan_store::seal_of(&b.world, &b.log),
            kuyuan_store::seal_of(&a.world, &a.log),
            "续跑封条应逐字节相等（存→读→续 ≡ 不中断）"
        );
    }

    /// 存档被篡改：版本／指纹／拍数／输入——该拒的拒。
    #[test]
    fn tampered_archives_are_rejected() {
        let s = Session::new(PACK, 7).expect("开局");
        let save = s.save_json().expect("应能存档");
        assert!(Session::load(PACK, &save).is_ok(), "原档应能读");

        let mut v = save.clone();
        v["标准"] = serde_json::json!("库元/play/存档/v9");
        assert!(Session::load(PACK, &v).is_err(), "版本不符应拒绝");

        let mut v = save.clone();
        v["存档"]["指纹"] = serde_json::json!("fnv1a64:0000000000000000");
        assert!(Session::load(PACK, &v).is_err(), "指纹不符应拒绝");

        let mut v = save.clone();
        let t0 = v["存档"]["拍"].as_u64().unwrap_or(1);
        v["存档"]["拍"] = serde_json::json!(t0.saturating_sub(1));
        assert!(Session::load(PACK, &v).is_err(), "拍数不符应拒绝");

        let mut v = save.clone();
        v["存档"]["输入"][0]["写"]["duel32.玩家1.出生x"] = serde_json::json!(11.1);
        assert!(Session::load(PACK, &v).is_err(), "输入被篡改应拒绝");
    }

    /// 开局落位：界内 · 不重叠（下限 1 格）。
    #[test]
    fn spawn_is_bounded_and_separated() {
        for seed in [1u64, 2, 3, 42, 99] {
            let s = Session::new(PACK, seed).expect("开局");
            let p1 = (s.vf(&s.ps("玩家1", "x")), s.vf(&s.ps("玩家1", "y")));
            let p2 = (s.vf(&s.ps("玩家2", "x")), s.vf(&s.ps("玩家2", "y")));
            for p in [p1, p2] {
                assert!(
                    (0.0..=32.0).contains(&p.0) && (0.0..=32.0).contains(&p.1),
                    "落点应在界内: {p:?}"
                );
            }
            let d = ((p1.0 - p2.0).powi(2) + (p1.1 - p2.1).powi(2)).sqrt();
            assert!(d >= 1.0, "双方落位距离 {d} < 1");
        }
    }
}
