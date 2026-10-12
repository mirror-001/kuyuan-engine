//! 库元引擎 · build —— 物化器（声明 → 实例）。
//! S3 转实：槽位面（值表 ＋ 编译后的公式）；引用在场校验；公式单写者（《引擎·标准》§1 · §4 · §10·3）。
//! 收口（裁 #30）：增**类型门禁**——算子／分支／结果 vs 目标声明（§12 表 19）。
//!
//! 本件只管**值面**：起手值、被接线输入的落座、公式实例。
//! 地址面在 `registry`，关系面在 `graph`。

use std::collections::{BTreeMap, BTreeSet};

use kuyuan_core::{parse_expr, Pack, Value};
use kuyuan_eval::{compile, type_of, unresolved_addrs, CExpr};
use kuyuan_graph::Graph;
use kuyuan_registry::{type_ok, Registry};

/// 公式实例：目标槽位 ＋ 编译后的表达式（引用已解析为槽位）。
pub struct FormulaInst {
    pub name: String,
    /// 目标槽位（必为变量）。
    pub target: usize,
    pub expr: CExpr,
}

/// 实例：槽位面 ＋ 接线（供驱动期搬运）。
pub struct Instance {
    /// 值表（与 `registry::Registry::addr_of` 同序）。
    pub slots: Vec<Value>,
    pub formulas: Vec<FormulaInst>,
    /// 接线：源槽位 → 目标槽位（目标必为输入）。
    pub wires: Vec<(usize, usize)>,
    /// 被接线的输入槽位＝引擎内部信号（拒外部注入；§11·2 裁 #24）。
    pub wired_inputs: BTreeSet<usize>,
    ///界式（S9；§14·3）：与 `addr_of` 同序——`Some` ＝该输入挂了「界」（编译后 AST，供注入口判定）。
    pub bounds: Vec<Option<CExpr>>,
}

/// 物化：未定义检查 → 起手值 → 公式（未知标签／悬空引用／单写者）。
/// 校验顺序与错误措辞与 S3 前一致，只把**悬空引用**从运行期提前到加载期。
pub fn materialize(pack: &Pack, reg: &Registry, net: &Graph) -> Result<Instance, String> {
    // ① 输入未接线且无默认＝未定义（§3）。
    let wired_addrs: BTreeSet<&str> =
        net.wires.iter().map(|(_, t)| reg.addr_of[*t].as_str()).collect();
    for a in &reg.inputs {
        if !reg.has_default.contains(a) && !wired_addrs.contains(a.as_str()) {
            return Err(format!("输入未接线且无默认: {a}（未定义）"));
        }
    }

    // ② 起手值：变量初值 ＋ 输入默认；被接线的输入以**源之初值**落座（t0 即"已接"，免得占位噪声）。
    //    源必不是被接线的输入（`graph` 已拒链），故落座与声明顺序无关。
    let mut values: BTreeMap<&str, Value> =
        reg.inits.iter().map(|(a, v)| (a.as_str(), v.clone())).collect();
    let src: Vec<Value> =
        net.wires.iter().map(|(f, _)| values[reg.addr_of[*f].as_str()].clone()).collect();
    for (i, (_, t)) in net.wires.iter().enumerate() {
        values.insert(reg.addr_of[*t].as_str(), src[i].clone());
    }
    let slots: Vec<Value> = reg
        .addr_of
        .iter()
        .map(|a| values.get(a.as_str()).cloned().unwrap_or(Value::Int(0)))
        .collect();

    // ③ 公式：目标是变量 ＋ 标签认识 ＋ 引用在场 ＋ 目标单写者。
    let mut formulas: Vec<FormulaInst> = vec![];
    let mut writers: BTreeMap<usize, usize> = BTreeMap::new();
    // 类型门禁（§12 表 19）要再过一遍表达式，故把解析结果留下：`(公式全名, 目标地址, 表达式)`。
    let mut typed: Vec<(String, String, kuyuan_core::Expr)> = vec![];
    for m in &pack.modules {
        for f in &m.formulas {
            let target_addr = format!("{}.{}", m.id, f.target);
            if !reg.vars.contains(&target_addr) {
                return Err(format!("公式目标不是变量: {target_addr}"));
            }
            let target = *reg
                .index
                .get(&target_addr)
                .ok_or_else(|| format!("地址不在场: {target_addr}"))?;
            *writers.entry(target).or_insert(0) += 1;
            let expr = parse_expr(&f.expr)?;
            let c = compile(&expr, &reg.index);
            // §1 名字是地址：引用不在场＝拒绝加载（S3 前拖到运行期才报）。
            if let Some(addr) = unresolved_addrs(&c).into_iter().next() {
                return Err(format!("引用不存在: {addr}（拒绝加载）"));
            }
            typed.push((format!("{}.{}", m.id, f.name), target_addr.clone(), expr));
            formulas.push(FormulaInst { name: format!("{}.{}", m.id, f.name), target, expr: c });
        }
    }
        //③′ 界式（S9；§14·3）：输入可挂 `"界"` ＝ 布尔表达式——标签认识（§12·13）· 引用在场（§12·14），
        //    类型须布尔并入第四节（§12·19）；与公式同一批门禁，仍「只报第一处」。
        let mut bounds: Vec<Option<CExpr>> = (0..reg.addr_of.len()).map(|_| None).collect();
        let mut typed_bounds: Vec<(String, kuyuan_core::Expr)> = vec![];
        for m in &pack.modules {
            for p in &m.inputs {
                if let Some(b) = &p.bound {
                    let addr = format!("{}.{}", m.id, p.name);
                    let expr = parse_expr(b)?;
                    let c = compile(&expr, &reg.index);
                    if let Some(a) = unresolved_addrs(&c).into_iter().next() {
                        return Err(format!("引用不存在: {a}（拒绝加载）"));
                    }
                    let slot = *reg.index.get(&addr).ok_or_else(|| format!("地址不在场: {addr}"))?;
                    bounds[slot] = Some(c);
                    typed_bounds.push((addr, expr));
                }
            }
        }

    if let Some((t, _)) = writers.iter().find(|e| *e.1 > 1) {
        return Err(format!("同一目标多个写者: {} —— 拒绝加载", reg.addr_of[*t]));
    }

    // ④ 类型相容（§12 表 19；裁 #30 收口）：算子／分支／**结果 vs 目标声明**——照运行期语义静态推。
    //    排在最后一遍，与 §12 表序一致（19 在 11–15 之后），故仍是「只报第一处」。
    for (name, target_addr, expr) in &typed {
        let inferred = type_of(expr, &reg.types)
            .map_err(|why| format!("类型不符: 公式 {name}: {why} —— 拒绝加载"))?;
        let declared = reg.types.get(target_addr).expect("目标已验为变量");
        if !type_ok(declared, &inferred) {
            return Err(format!(
                "类型不符: 公式 {name} 结果「{inferred}」≠ 目标 {target_addr} 声明「{declared}」—— 拒绝加载"
            ));
        }
    }

        //界式类型（§12·19 顺延；S9）：须为布尔——静态推一遍，与运行期判定同语义。
        for (addr, expr) in &typed_bounds {
            let inferred = type_of(expr, &reg.types)
                .map_err(|why| format!("类型不符:界式 {addr}: {why} ——拒绝加载"))?;
            if inferred != "布尔" {
                return Err(format!(
                    "类型不符:界式 {addr}结果「{inferred}」≠须为「布尔」——拒绝加载"
                ));
            }
        }

    Ok(Instance {
        slots,
        formulas,
        wires: net.wires.clone(),
        wired_inputs: net.wired_inputs.clone(),
            bounds,
        })
}
