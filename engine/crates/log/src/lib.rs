//! 库元引擎 · log —— 审计日志 / 事件（唯一事实源 · 只追加）。
//! S4：引擎事件四类（《引擎·标准》§7.1）＋ 拒绝事件（§7.2）。
//! 收口：拒绝事件带**来源**字段（`加载`／`运行期`）——不再靠 `tick＝0` ＋ 目标＝包名**隐含**（裁 #31）。
//! 留痕批：本件即构成四样之「**留痕**」（记"世界曾是什么"）；兼作**拍序的保管者**——
//!         `check_tick` 只放行**恰好下一拍**，重复／跳号＝拒绝推进（《引擎·标准》§10·3）。
//! S8：增 `fork`——分支新账（回滚＝另起分支，§13·4）。
use kuyuan_core::{json_of_value, Value};
use serde::Serialize;

/// 事件类型：**公式**写回变量。
pub const KIND_VAR: &str = "变量变更";
/// 事件类型：**外部**经输入集写入口。
pub const KIND_INPUT: &str = "输入注入";
/// 事件类型：**接线**把源值搬到目标输入。
pub const KIND_CARRY: &str = "搬运";
/// 事件类型：不被接受的输入／加载失败（§7.2）。
pub const KIND_REJECT: &str = "拒绝";

/// 拒绝**来源**：加载期（`tick＝0`，目标通常是包名）。
pub const SOURCE_LOAD: &str = "加载";
/// 拒绝**来源**：运行期（有 tick，目标是地址）。
pub const SOURCE_RUN: &str = "运行期";

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    #[serde(rename = "序")] pub seq: u64,
    #[serde(rename = "tick")] pub tick: u64,
    #[serde(rename = "类型")] pub kind: String,
    #[serde(rename = "目标")] pub target: String,
    #[serde(rename = "旧")] pub old: serde_json::Value,
    #[serde(rename = "新")] pub new: serde_json::Value,
    /// 仅「拒绝」事件有（裁 #31）：`加载`／`运行期`。其余事件**不写**此字段——故旧输出逐字节不变。
    #[serde(rename = "来源", skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Default)]
pub struct EventLog {
    pub events: Vec<Event>,
    next_seq: u64,
    /// 账本自己的时间：已成功推进到的**最后一拍**（起手 0＝尚未推进；分支新账＝`fork` 的起点拍）。
    /// 留痕是拍序的保管者（§10·3）——此前只有调用方在守拍序。
    pub last_tick: u64,
}

impl EventLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// **分支**（§13·4；裁 #35）：开一本**新账**，从第 `tick` 拍起步——
    /// 只记此后；前史留在源账里，**不抄、不截断、不篡改**（回滚＝另起分支的落点）。
    pub fn fork(tick: u64) -> Self {
        Self { events: Vec::new(), next_seq: 0, last_tick: tick }
    }

    /// **拍序校验**（§10·3；留痕批）：只放行**恰好下一拍**（首拍为 1）。
    /// 重复或跳号 → **记一条「拒绝」再 Err**（§7.2：拒绝是发生过的事实）：
    /// 账本若对拍序沉默，"唯一事实源"就只是口号。
    /// 边界：**管序不管限**——"最多跑多少拍"是守护（`guard`）的时间界〔后置〕。
    pub fn check_tick(&mut self, tick: u64) -> Result<(), String> {
        let expect = self.last_tick + 1;
        if tick == expect {
            return Ok(());
        }
        let reason =
            format!("拍序不符: 期望第 {expect} 拍，实得第 {tick} 拍 —— 拒绝推进");
        self.push_reject(tick, SOURCE_RUN, "拍", &reason);
        Err(reason)
    }

    /// 记一条「变量变更」；值未变则不记。
    pub fn record(&mut self, tick: u64, target: &str, old: &Value, new: &Value) {
        self.record_kind(tick, KIND_VAR, target, old, new);
    }

    /// 记一条指定类型的**状态**事件（旧≠新才记）——事件分类的落点（§7.1）。
    /// 比较用 `Value::same_as`：非有限实数**按表示**比（§10·4），故 `NaN` 不会每拍刷假事件。
    pub fn record_kind(&mut self, tick: u64, kind: &str, target: &str, old: &Value, new: &Value) {
        if old.same_as(new) {
            return;
        }
        self.events.push(Event {
            seq: self.next_seq,
            tick,
            kind: kind.to_string(),
            target: target.to_string(),
            old: json_of_value(old),
            new: json_of_value(new),
            source: None,
        });
        self.next_seq += 1;
    }

    /// 记一条**加载期**「拒绝」（§7.2／§8）：`目标` ＝ 包名或地址；加载期无 tick，记 `tick = 0`。
    pub fn reject_load(&mut self, target: &str, reason: &str) {
        self.push_reject(0, SOURCE_LOAD, target, reason);
    }

    /// 记一条**运行期**「拒绝」（§7.2／§10·7）：`目标` ＝ 被拒的地址。
    pub fn reject_run(&mut self, tick: u64, target: &str, reason: &str) {
        self.push_reject(tick, SOURCE_RUN, target, reason);
    }

    /// 「拒绝」的落点：`reason` ＝ 拒绝理由原文（入「新」）。
    /// **不受"旧≠新"约束**——拒绝是发生过的事实，不是状态变更。
    fn push_reject(&mut self, tick: u64, source: &str, target: &str, reason: &str) {
        self.events.push(Event {
            seq: self.next_seq,
            tick,
            kind: KIND_REJECT.to_string(),
            target: target.to_string(),
            old: serde_json::Value::Null,
            new: serde_json::Value::String(reason.to_string()),
            source: Some(source.to_string()),
        });
        self.next_seq += 1;
    }
}
