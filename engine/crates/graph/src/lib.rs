//! 库元引擎 · graph —— 关系网（接线 · 组成 · 触发）。
//! S3 转实：关系白名单 ＋ 接线校验（单一写者 · 无接线链）（《引擎·标准》§5 · §10·3）。
//!
//! 本件只管**关系面**：谁写给谁。值不在这里（见 `build`），时间不在这里（见 `driver`）。

use std::collections::BTreeSet;

use kuyuan_core::Pack;
use kuyuan_registry::Registry;

/// 关系白名单（§5）：`接线`（运算·恒等搬运）· `组成`（结构·骨架）· `触发`（预留）。
const KINDS: [&str; 3] = ["接线", "组成", "触发"];

/// 关系网：接线边 ＋ 结构声明。
pub struct Graph {
    /// 接线：源槽位 → 目标槽位（目标必为输入）。
    pub wires: Vec<(usize, usize)>,
    /// 被接线的输入槽位＝引擎内部信号（拒外部注入；§11·2 裁 #24）。
    pub wired_inputs: BTreeSet<usize>,
    /// 组成（结构声明，不运算）：父 → 子。
    pub compositions: Vec<(String, String)>,
}

/// 建关系网：白名单 → 接线校验 → 无链（两遍：先立边，再查链）。
pub fn wire(pack: &Pack, reg: &Registry) -> Result<Graph, String> {
    let mut wires: Vec<(usize, usize)> = vec![];
    let mut wired_inputs: BTreeSet<usize> = BTreeSet::new();
    let mut compositions: Vec<(String, String)> = vec![];
    // 目标 → 源地址：单一写者判定（§10·3 覆盖接线，不只公式）。
    let mut writer_of: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

    for r in &pack.relations {
        match r.kind.as_str() {
            "接线" => {
                let from = r.from.clone().ok_or("接线缺“从”")?;
                let to = r.to.clone().ok_or("接线缺“到”")?;
                let f = *reg
                    .index
                    .get(&from)
                    .ok_or_else(|| format!("接线源不存在: {from}"))?;
                let t = match reg.index.get(&to) {
                    Some(i) if reg.inputs.contains(&to) => *i,
                    _ => return Err(format!("接线目标不是输入: {to}")),
                };
                if let Some(prev) = writer_of.insert(to.clone(), from.clone()) {
                    return Err(format!(
                        "同一输入多个写者: {to}（已由「{prev}」接线）—— 拒绝加载（§10·3 单写者）"
                    ));
                }
                wires.push((f, t));
                wired_inputs.insert(t);
            }
            "组成" => {
                let parent = r.parent.clone().ok_or("组成缺“父”")?;
                let child = r.child.clone().ok_or("组成缺“子”")?;
                compositions.push((parent, child));
            }
            // 预留（§5）：声明了却未实现——拒，不静默忽略。
            "触发" => return Err("关系类型「触发」未实现（后置）—— 拒绝加载".into()),
            other => {
                return Err(format!(
                    "未知关系类型: {other}（白名单：{}）—— 拒绝加载",
                    KINDS.join("／")
                ));
            }
        }
    }

    // 第二遍：**无接线链**（裁 #29）。源可以是输入（入口扇出——duel32 的「动作 → 各探.令动作」，
    // 实测 132 条里 24 条如此），但**源自己不得是被接线的输入**：那是引擎内部信号，
    // 再接一层既非「从输出搬运」，又让 t0 落座依赖声明顺序、运行时每级再滞后一拍。
    let targets: BTreeSet<String> = writer_of.keys().cloned().collect();
    for (to, from) in &writer_of {
        if targets.contains(from) {
            return Err(format!(
                "接线链（源「{from}」本身已被接线，却再接给 {to}）—— 拒绝加载；请直接从源头接线（§5）"
            ));
        }
    }

    Ok(Graph { wires, wired_inputs, compositions })
}
