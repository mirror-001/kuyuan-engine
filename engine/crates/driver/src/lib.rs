//! 库元引擎 · driver —— 驱动器 / 时钟（定步 · 确定性 · 同步单遍）。
//! S3：`load` 三拆——**注册（registry）→ 接线（graph）→ 物化（build）**；本件只做编排与推进。
//! S4：事件分类（变量变更／输入注入／搬运／拒绝）＋ **拒绝入日志**（《引擎·标准》§7.1–7.2）。
//! 留痕批：本件是**唯一改值现场**——每次改值必留痕（三种写者都经 `log` 的私有闸门）；
//!         推进入口先过 `log.check_tick`（拍序由留痕保管，§10·3）。
//! S2b：世界槽位化——地址 ↔ 索引；求值不再做字符串查找。语义与 S2a 前逐字节一致。
//! S8（#42＝乙）：`World` 增「拍」——只供对账（§13·3），不进快照、不进求值、不进事件。
use std::collections::{BTreeMap, BTreeSet};

use kuyuan_core::{Pack, Value, Vis};
use kuyuan_eval::{eval_c, CExpr};
use kuyuan_log::{EventLog, KIND_CARRY, KIND_INPUT};

pub use kuyuan_build::FormulaInst;

pub struct World {
    /// 地址面：输入（外部可写）· 变量（公式可写）——运行期可分（S2a）。
    pub input_ids: BTreeSet<String>,
    pub var_ids: BTreeSet<String>,
    /// 槽位面（S2b）：槽位 ↔ 地址 双向对照 ＋ 值表（键升序，与快照同序）。
    pub addr_of: Vec<String>,
    pub index: BTreeMap<String, usize>,
    pub slots: Vec<Value>,
    pub formulas: Vec<FormulaInst>,
    ///类型面（S9；§14·2）：与 `addr_of` 同序的**声明类型**——注入类型核对的依据。
    pub types: Vec<String>,
    ///界式（S9；§14·3）：与 `addr_of` 同序——`Some` ＝该输入挂了「界」（编译后 AST）。
    pub bounds: Vec<Option<CExpr>>,
    /// 接线：源槽位 → 目标槽位（目标必为输入）。
    pub wires: Vec<(usize, usize)>,
    /// 被接线的输入槽位＝引擎内部信号，拒绝外部注入（§11 裁 #24）。
    pub wired_inputs: BTreeSet<usize>,
    /// 可见性面（§10·5；S7）：与 `addr_of` 同序的**档位**与**归属者**。
    /// 只供观测投影（`crates/view`）——不进求值、不进事件，故**视角不改世界**。
    pub vis: Vec<Vis>,
    pub owner: Vec<Option<String>>,
    /// 「拍」：已成功推进到的**最后一拍**（起手 0）——S8 增（裁 #42＝乙）。
    /// **只供对账**（§13·3：`世界.tick ＝ 账本.last_tick`），不进快照、不进求值、不进事件。
    pub tick: u64,
}

/// 加载 ＝ **编排三件**（S3 三拆，拒绝优于猜）：
/// ① `registry::register` 登记地址面与类型（§1／§3／§8）＋可见性面（§10·5；S7）；
/// ② `graph::wire` 建关系面并校验（§5／§10·3）；
/// ③ `build::materialize` 建值面与公式实例（§4 标签 · §1 引用在场 · §3 未定义）。
pub fn load(pack: &Pack) -> Result<World, String> {
    let reg = kuyuan_registry::register(pack)?;
    let net = kuyuan_graph::wire(pack, &reg)?;
    let inst = kuyuan_build::materialize(pack, &reg, &net)?;
    // 可见性面（S7）：注册表按**地址**记档，物化成与槽位**同序**的数组——观测侧按槽位取档。
    let vis: Vec<Vis> = reg.addr_of.iter().map(|a| *reg.vis.get(a).unwrap_or(&Vis::Open)).collect();
        let owner: Vec<Option<String>> =
        reg.addr_of.iter().map(|a| reg.owner.get(a).cloned()).collect();
    //类型面（S9）：与 `addr_of` 同序的声明类型（registry 已全量登记，直接取）。
    let types: Vec<String> = reg.addr_of.iter().map(|a| reg.types[a].clone()).collect();
    Ok(World {
        input_ids: reg.inputs,
        var_ids: reg.vars,
        addr_of: reg.addr_of,
        index: reg.index,
        slots: inst.slots,
        formulas: inst.formulas,
        wires: inst.wires,
        wired_inputs: inst.wired_inputs,
        vis,
        owner,
        types,
        bounds: inst.bounds,
        tick: 0,
    })
}

/// 加载（**记日志的形**，S4）：被拒时向 `log` 写一条「拒绝」（§7.2／§8）。
/// 加载期无 tick，记 `tick = 0`；`目标` ＝ 包名。
pub fn load_logged(pack: &Pack, log: &mut EventLog) -> Result<World, String> {
    match load(pack) {
        Ok(w) => Ok(w),
        Err(e) => {
            log.reject_load(&pack.ns, &e);
            Err(e)
        }
    }
}

impl World {
    /// 该地址是不是输入（外部可写）。
    pub fn is_input(&self, addr: &str) -> bool {
        self.input_ids.contains(addr)
    }

    /// 该地址是不是变量（公式可写）。
    pub fn is_var(&self, addr: &str) -> bool {
        self.var_ids.contains(addr)
    }

    /// 输入地址表（升序）。
    pub fn inputs(&self) -> impl Iterator<Item = &String> {
        self.input_ids.iter()
    }

    /// 变量地址表（升序）。
    pub fn vars(&self) -> impl Iterator<Item = &String> {
        self.var_ids.iter()
    }

    /// 取某地址之值。
    pub fn get(&self, addr: &str) -> Option<&Value> {
        self.index.get(addr).map(|i| &self.slots[*i])
    }

    /// 一拍：读起点 → 搬运 → **注入** → 求值 → 原子写回 → 记日志。
    pub fn tick_in(
        &mut self,
        tick: u64,
        inputs: &BTreeMap<String, Value>,
        log: &mut EventLog,
    ) -> Result<(), String> {
        // **拍序先行**（§10·3；留痕批）：账本兼作拍序的保管者——重复／跳号＝拒绝推进（记「拒绝」）。
        // 它不触发状态变更，故排在注入校验之前：拍序不对，本拍根本没发生。
        log.check_tick(tick)?;

        // 输入校验（§11；裁 #24：被接线的输入＝引擎内部信号，拒绝外部注入）
        // S4：被拒**先记一条「拒绝」再返回 Err**（§10·7／§7.2）——地址升序，只报第一处。
        for addr in inputs.keys() {
            let reason = match self.index.get(addr) {
                None => Some(format!("输入地址不在场: {addr}")),
                Some(i) => {
                    if !self.input_ids.contains(addr) {
                        Some(format!("不是输入（变量由公式写）: {addr}"))
                    } else if self.wired_inputs.contains(i) {
                        Some(format!("输入已被接线（引擎内部信号，拒绝外部注入）: {addr}"))
                    } else {
                        None
                    }
                }
            };
            if let Some(reason) = reason {
                log.reject_run(tick, addr, &reason);
                return Err(reason);
            }
        }

        let mut snap = self.slots.clone();
        // 注入先行：设定入口——「本拍初值」＝ tick 初值 ＋ 注入（§11·5）
        for (addr, v) in inputs {
            let i = *self.index.get(addr).ok_or_else(|| format!("输入地址不在场: {addr}"))?;
            snap[i] = v.clone();
        }
        //新两道（S9；§14·2／§14·3）：先类型（全帧）、后界式（读本拍初值）——
        //与三拒同段、同形：先记「拒绝」再 Err，只报第一处（§11 规则 7–8）。
        let mut shots: Vec<kuyuan_guard::Shot> = Vec::with_capacity(inputs.len());
        for (addr, v) in inputs {
            let i = *self.index.get(addr).ok_or_else(|| format!("输入地址不在场: {addr}"))?;
            shots.push(kuyuan_guard::Shot {
                addr: addr.as_str(),
                value: v,
                declared: &self.types[i],
                bound: self.bounds[i].as_ref(),
            });
        }
        if let Err((target, reason)) = kuyuan_guard::check_inject(&shots, &snap) {
            log.reject_run(tick, &target, &reason);
            return Err(reason);
        }

        // 搬运：源读「本拍初值」，不读本拍搬运结果——链式已被加载期拒绝（#29）
        let src: Vec<Value> = self.wires.iter().map(|(f, _)| snap[*f].clone()).collect();
        for (i, (_, to)) in self.wires.iter().enumerate() {
            snap[*to] = src[i].clone();
        }

        let mut writes: Vec<(usize, Value)> = Vec::with_capacity(self.formulas.len());
        for f in &self.formulas {
            match eval_c(&f.expr, &snap) {
                Ok(v) => writes.push((f.target, v)),
                Err(e) => {
                    // 求值被拒（整数除零／溢出／类型错）→ 记「拒绝」（§10·7），目标＝该公式要写的地址。
                    let reason = format!("公式 {}: {e}", f.name);
                    log.reject_run(tick, &self.addr_of[f.target], &reason);
                    return Err(reason);
                }
            }
        }

        for (_, to) in &self.wires {
            let new = snap[*to].clone();
            let old = self.slots[*to].clone();
            self.slots[*to] = new.clone();
            log.record_kind(tick, KIND_CARRY, &self.addr_of[*to], &old, &new);
        }
        for (addr, v) in inputs {
            let i = *self.index.get(addr).ok_or_else(|| format!("输入地址不在场: {addr}"))?;
            let old = self.slots[i].clone();
            self.slots[i] = v.clone();
            log.record_kind(tick, KIND_INPUT, &self.addr_of[i], &old, v);
        }
        for (t, new) in writes {
            let old = self.slots[t].clone();
            self.slots[t] = new.clone();
            log.record(tick, &self.addr_of[t], &old, &new);
        }
        // 本拍已落账：账本时间前进 ＋ 世界「拍」跟上（对账两腿，§13·3；拍序的唯一写入点）。
        log.last_tick = tick;
        self.tick = tick;
        Ok(())
    }

    /// 一拍（无输入）：`tick_in` 的空输入形。
    pub fn tick(&mut self, tick: u64, log: &mut EventLog) -> Result<(), String> {
        self.tick_in(tick, &BTreeMap::new(), log)
    }

    pub fn snapshot_json(&self) -> String {
        let map: BTreeMap<&String, serde_json::Value> = self
            .addr_of
            .iter()
            .enumerate()
            .map(|(i, a)| (a, kuyuan_core::json_of_value(&self.slots[i])))
            .collect();
        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".into())
    }
}
