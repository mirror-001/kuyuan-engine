//! 库元引擎 · registry —— 注册表（名字是地址）。
//! S3 转实：地址面登记 ＋ 类型声明校验（《引擎·标准》§1 命名 · §3 模块 · §8 门禁）。
//! S7 补：**可见性面**——三档（真／私／裁判）解析与校验（§10·5；§12 表 16–18）。
//!
//! 本件只管**地址面**：谁在场、什么类型、起手何值、谁能看见。
//! 不碰关系（见 `graph`）、不建槽位（见 `build`）、不做投影（见 `view`）。

use std::collections::{BTreeMap, BTreeSet};

use kuyuan_core::{value_of_json, vis_of_name, Pack, Value, Vis, JUDGE};

/// 类型最小集（§3）：`引用` 后置。
pub const TYPES: [&str; 4] = ["整数", "实数", "布尔", "文本"];

/// 注册表：地址 ↔ 槽位序号 ＋ 类型 ＋ 起手值。
pub struct Registry {
    /// 地址表（升序；快照与事件流同序）。
    pub addr_of: Vec<String>,
    /// 地址 → 槽位序号。
    pub index: BTreeMap<String, usize>,
    /// 变量地址（公式可写）。
    pub vars: BTreeSet<String>,
    /// 输入地址（外部可写；被接线者除外——接线归引擎，见 `graph`）。
    pub inputs: BTreeSet<String>,
    /// 地址 → 声明类型（§3）。
    pub types: BTreeMap<String, String>,
    /// 地址 → 起手值（变量＝初值；输入＝默认值）。
    pub inits: BTreeMap<String, Value>,
    /// 带默认值的输入——「未接线且无默认」判定的依据（§3）。
    pub has_default: BTreeSet<String>,
    /// 模块 id（按声明序）。
    pub modules: Vec<String>,
    /// 地址 → 可见性档（§10·5；S7）：逐条声明 → 模块默认 → 缺省 `真`。
    pub vis: BTreeMap<String, Vis>,
    /// 地址 → 归属者（模块声明了 `归属` 时才有；`私` 档的可见对象）。
    pub owner: BTreeMap<String, String>,
}

impl Registry {
    /// 该地址声明类型（不在场＝None）。
    pub fn type_of(&self, addr: &str) -> Option<&String> {
        self.types.get(addr)
    }
}

/// 档名不认识的措辞（§12 表 16）。
fn unknown_vis(what: &str, s: &str) -> String {
    format!("未知可见性: {what} 声明「{s}」（三档：真／私／裁判）—— 拒绝加载")
}

/// 类型相容（§3）：同名相容；**整数可入实数**（无损放宽——32×32 有 60 个实数字段的初值写作整数）；反向不容。
/// `build` 的**类型门禁**（§12 表 19）复用同一条规则，免得两处判据走散。
pub fn type_ok(declared: &str, actual: &str) -> bool {
    declared == actual || (declared == "实数" && actual == "整数")
}

/// 声明值校验：类型名在白名单内 ＋ 与声明相容。`what` ＝ "初值" / "默认"。
fn declared_value(
    addr: &str,
    ty: &str,
    what: &str,
    v: &serde_json::Value,
) -> Result<Value, String> {
    if !TYPES.contains(&ty) {
        return Err(format!(
            "未知类型: {addr} 声明「{ty}」（类型最小集：整数／实数／布尔／文本）—— 拒绝加载"
        ));
    }
    let val = value_of_json(v)?;
    if !type_ok(ty, val.type_name()) {
        return Err(format!(
            "{what}类型不符: {addr} 声明「{ty}」，{what}却是「{}」—— 拒绝加载",
            val.type_name()
        ));
    }
    Ok(val)
}

/// 登记一个包：门禁（§8）→ 地址面 → 类型声明。
pub fn register(pack: &Pack) -> Result<Registry, String> {
    if !pack.std_version.starts_with("库元/引擎/") {
        return Err(format!("标准版本不认识: {}", pack.std_version));
    }
    if pack.gate.refutable.trim().is_empty()
        || pack.gate.domain.trim().is_empty()
        || pack.gate.checker.trim().is_empty()
    {
        return Err("三问缺项（拒绝加载）".into());
    }

    // 声明过的一切地址——含**无默认的输入**（它们在 build 期才因接线而落座）。
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut values: BTreeMap<String, Value> = BTreeMap::new();
    let mut vars: BTreeSet<String> = BTreeSet::new();
    let mut inputs: BTreeSet<String> = BTreeSet::new();
    let mut types: BTreeMap<String, String> = BTreeMap::new();
    let mut has_default: BTreeSet<String> = BTreeSet::new();
    let mut modules: Vec<String> = Vec::new();
    let mut mod_ids: BTreeSet<String> = BTreeSet::new();

    for m in &pack.modules {
        if !mod_ids.insert(m.id.clone()) {
            return Err(format!("模块 id 重复: {}（名字是地址）", m.id));
        }
        modules.push(m.id.clone());

        for v in &m.vars {
            let a = format!("{}.{}", m.id, v.name);
            if !declared.insert(a.clone()) {
                return Err(format!("地址重复: {a}"));
            }
            let val = declared_value(&a, &v.ty, "初值", &v.init)?;
            types.insert(a.clone(), v.ty.clone());
            values.insert(a.clone(), val);
            vars.insert(a);
        }
        for p in &m.inputs {
            let a = format!("{}.{}", m.id, p.name);
            if !declared.insert(a.clone()) {
                return Err(format!("地址重复: {a}"));
            }
            types.insert(a.clone(), p.ty.clone());
            match &p.default {
                Some(d) => {
                    let val = declared_value(&a, &p.ty, "默认", d)?;
                    values.insert(a.clone(), val);
                    has_default.insert(a.clone());
                }
                // 无默认也要校验类型名：声明本身即错，不该等到物化期。
                None => {
                    if !TYPES.contains(&p.ty.as_str()) {
                        return Err(format!(
                            "未知类型: {a} 声明「{}」（类型最小集：整数／实数／布尔／文本）—— 拒绝加载",
                            p.ty
                        ));
                    }
                }
            }
            inputs.insert(a);
        }
    }

    // ③ 可见性面（§10·5；S7）：**16 档名认识 → 17 私必有归属 → 18 归属不占保留名**，
    //    三遍各走模块声明序——与 §12 表同序，故「只报第一处」。
    //    只管变量与输入（`输出` 表不参与判定）；逐条声明 → 模块默认 → 缺省「真」。
    let mut resolved: Vec<(String, Vis, Option<String>, String)> = Vec::new();
    for m in &pack.modules {
        let default = match &m.vis {
            Some(s) => vis_of_name(s).ok_or_else(|| unknown_vis(&format!("模块 {}", m.id), s))?,
            None => Vis::Open,
        };
        let declared_here = m
            .vars
            .iter()
            .map(|v| (&v.name, &v.vis))
            .chain(m.inputs.iter().map(|p| (&p.name, &p.vis)));
        for (name, declared) in declared_here {
            let addr = format!("{}.{}", m.id, name);
            let tier = match declared {
                Some(s) => vis_of_name(s).ok_or_else(|| unknown_vis(&addr, s))?,
                None => default,
            };
            resolved.push((addr, tier, m.owner.clone(), m.id.clone()));
        }
    }
    for (addr, tier, owner, id) in &resolved {
        if *tier == Vis::Owner && owner.is_none() {
            return Err(format!("私 档缺归属: {addr}（模块 {id} 未声明「归属」）—— 拒绝加载"));
        }
    }
    for m in &pack.modules {
        if m.owner.as_deref() == Some(JUDGE) {
            return Err(format!(
                "归属占用保留名: 模块 {} 的「归属」＝{JUDGE}（裁判＝全见，保留名）—— 拒绝加载",
                m.id
            ));
        }
    }
    let vis: BTreeMap<String, Vis> = resolved.iter().map(|(a, t, _, _)| (a.clone(), *t)).collect();
    let owner: BTreeMap<String, String> = resolved
        .iter()
        .filter_map(|(a, _, o, _)| o.clone().map(|o| (a.clone(), o)))
        .collect();

    let addr_of: Vec<String> = declared.into_iter().collect();
    let index: BTreeMap<String, usize> = addr_of
        .iter()
        .enumerate()
        .map(|(i, a)| (a.clone(), i))
        .collect();
    Ok(Registry {
        addr_of,
        index,
        vars,
        inputs,
        types,
        inits: values,
        has_default,
        modules,
        vis,
        owner,
    })
}
