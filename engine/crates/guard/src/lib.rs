//! 库元引擎 · guard —— 守护（四界）判定件（《引擎·标准》§14；S9）。
//! 「声明 → **判定** → 留痕」里的判定：注入两道（类型 · 界式）＋ 时间界（拍上限）。
//! 本件只判「放不放行」：声明住内容（§3「界」），留痕落 `log`（§7.2）。
//! - 注入两道：**不落痕**——调用方在 `tick_in` 里先记「拒绝」再 Err（§11 规则 7–8）；
//! - 拍上限：**先记「拒绝」再 Err**（与拍序同形；目标＝`拍` · 来源＝运行期）。
//!
//! 口径（裁 #44 · D8）：**不是**"引擎替模块守界"，而是「模块能声明自己的界，
//! 引擎负责判定并留痕」——本件就是那台「判定」。

use kuyuan_core::Value;
use kuyuan_eval::{eval_c, CExpr};
use kuyuan_log::EventLog;
use kuyuan_registry::type_ok;

/// 注入判定的一笔：地址 · 值 · 声明类型 · 界式（无界＝`None`）。
/// 调用方按**地址升序**备齐——判定序即注入序（§11 规则 7–8）。
pub struct Shot<'a> {
    pub addr: &'a str,
    pub value: &'a Value,
    pub declared: &'a str,
    pub bound: Option<&'a CExpr>,
}

/// 注入两道（§11 规则 7–8）：**先类型（全帧），后界式**——界式读「本拍初值」
/// （tick 初值 ＋ 本帧全部注入，§11·5）。只报第一处。
///
/// 返回 `Err((地址, 理由))`；**本件不落痕**——调用方先记「拒绝」再 Err（§7.2）。
pub fn check_inject(shots: &[Shot], snap: &[Value]) -> Result<(), (String, String)> {
    // 第一道：类型核对（§14·2）——相容律复用 registry（与加载期／求值期同一条）。
    for s in shots {
        if !type_ok(s.declared, s.value.type_name()) {
            let reason = format!(
                "注入类型不符: {}声明「{}」，注入却是「{}」——拒绝注入",
                s.addr,
                s.declared,
                s.value.type_name()
            );
            return Err((s.addr.to_string(), reason));
        }
    }
    // 第二道：界式（§14·3）——假 ＝ 越界；求值失败 ＝ 同拒（理由原文照转）。
    for s in shots {
        let Some(b) = s.bound else { continue };
        let verdict = match eval_c(b, snap) {
            Ok(v) => v,
            Err(e) => {
                let reason = format!("界式被拒: {e}——拒绝注入");
                return Err((s.addr.to_string(), reason));
            }
        };
        match verdict {
            Value::Bool(true) => {}
            Value::Bool(false) => {
                let reason = format!("越界注入: {}不满足「界」——拒绝注入", s.addr);
                return Err((s.addr.to_string(), reason));
            }
            other => {
                let reason = format!("界式被拒: 界式非布尔（{}）——拒绝注入", other.type_name());
                return Err((s.addr.to_string(), reason));
            }
        }
    }
    Ok(())
}

/// 时间界（§14·5）：推进尝试超过上限（实得拍号 > 上限）＝ **拒绝推进**——
/// **先记「拒绝」再 Err**（§7.2；与拍序同形：目标＝`拍` · 来源＝运行期）。
/// 缺省不调用＝**无界**（旧行为不变）。
pub fn check_limit(tick: u64, cap: u64, log: &mut EventLog) -> Result<(), String> {
    if tick <= cap {
        return Ok(());
    }
    let reason = format!("拍上限已达:上限 {cap} 拍，实得第 {tick} 拍 ——拒绝推进");
    log.reject_run(tick, "拍", &reason);
    Err(reason)
}