//! 库元引擎 · core —— 论域 / 类型 / Id。
//! M0：包模型（serde）＋ 值 / 表达式解析。契约见 [[引擎·标准]]。

use serde::{Deserialize, Serialize};

pub type Addr = String;

/// 值：整数 / 实数 / 布尔 / 文本（v0 类型最小集：裁 #3）
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Real(f64),
    Text(String),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "布尔",
            Value::Int(_) => "整数",
            Value::Real(_) => "实数",
            Value::Text(_) => "文本",
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Real(r) => Some(*r),
            _ => None,
        }
    }
    pub fn is_num(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Real(_))
    }
    /// 日志比较（§10·4）：两侧都是**非有限实数**时**按表示**比较（`NaN` 对 `NaN` ＝同值，
    /// 否则每拍都会刷一条 `null→null` 的假变更）；其余一律按**值**比较。
    pub fn same_as(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) if !a.is_finite() && !b.is_finite() => {
                real_marker(*a) == real_marker(*b)
            }
            _ => self == other,
        }
    }
}

/// 可见性三档（《引擎·标准》§10·5；裁 #9）：
/// `真`（双方＋裁判）／`私`（归属者＋裁判）／`裁判`（仅真实世界）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vis {
    /// 真：人人可见（缺省档）。
    Open,
    /// 私：归属者 ＋ 裁判。
    Owner,
    /// 裁判：仅裁判（真实世界）。
    Judge,
}

/// 视角**保留名**：`裁判` ＝全见（§10·5）。也是三档中 `裁判` 档的名字——同一串，两处同义。
pub const JUDGE: &str = "裁判";

/// 档名 → 档（不认识＝`None`：拒，不猜）。
pub fn vis_of_name(s: &str) -> Option<Vis> {
    match s {
        "真" => Some(Vis::Open),
        "私" => Some(Vis::Owner),
        "裁判" => Some(Vis::Judge),
        _ => None,
    }
}

/// 档 → 档名（错误措辞与报告用）。
pub fn vis_name(v: Vis) -> &'static str {
    match v {
        Vis::Open => "真",
        Vis::Owner => "私",
        Vis::Judge => "裁判",
    }
}

/// 字面（JSON）→ 值
pub fn value_of_json(v: &serde_json::Value) -> Result<Value, String> {
    match v {
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Real(f))
            } else {
                Err(format!("数字越界: {n}"))
            }
        }
        serde_json::Value::String(s) => Ok(Value::Text(s.clone())),
        // 非有限实数的**自描述形**（§10·4；裁 #32＝甲）：`{"实数":"NaN"}` → `Real(NaN)`。
        // 故「标记 → JSON → 实数」闭环无损，且读取端**无须**类型表（有限实数照旧写数字）。
        serde_json::Value::Object(m) => {
            if let Some(f) = m.get("实数").and_then(|x| x.as_str()).and_then(real_of_marker) {
                return Ok(Value::Real(f));
            }
            Err(format!("字面类型未支持: {v}"))
        }
        other => Err(format!("字面类型未支持: {other}")),
    }
}

/// 非有限实数的**文本标记**（§10·4；裁 #27＝乙）：`"NaN"`／`"inf"`／`"-inf"`。
/// 写 `null` 会让「初值＋事件流＝终态」（§7）在非有限值处断掉。
pub fn real_marker(f: f64) -> &'static str {
    if f.is_nan() {
        "NaN"
    } else if f > 0.0 {
        "inf"
    } else {
        "-inf"
    }
}

/// 标记**反解**（§10·4）：`"NaN"`／`"inf"`／`"-inf"` → 实数；其余 → `None`。故非有限值**无损**。
pub fn real_of_marker(s: &str) -> Option<f64> {
    match s {
        "NaN" => Some(f64::NAN),
        "inf" => Some(f64::INFINITY),
        "-inf" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

/// 值 → JSON（快照与事件共用）：非有限实数走**自描述形** `{"实数":"NaN"}`（§10·4；裁 #32＝甲），
/// 其余按原样。故**文本变量的 `"NaN"` 与实数 NaN 不再同形**——读取端无须类型表。
pub fn json_of_value(v: &Value) -> serde_json::Value {
    match v {
        Value::Real(f) if !f.is_finite() => serde_json::json!({ "实数": real_marker(*f) }),
        _ => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
    }
}

/// 最小公式 AST（v0.3.3：13 标签）
#[derive(Clone, Debug)]
pub enum Expr {
    Lit(Value),
    Ref(Addr),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Sqrt(Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
}

pub fn parse_expr(v: &serde_json::Value) -> Result<Expr, String> {
    let arr = v.as_array().ok_or_else(|| format!("表达式应为数组: {v}"))?;
    let tag = arr.first().and_then(|x| x.as_str()).ok_or("表达式缺标签")?;
    let arg = |i: usize| -> Result<&serde_json::Value, String> {
        arr.get(i).ok_or_else(|| format!("{tag}: 缺第 {i} 个参数"))
    };
    let bx = |i: usize| -> Result<Box<Expr>, String> { Ok(Box::new(parse_expr(arg(i)?)?)) };
    Ok(match tag {
        "字面" => Expr::Lit(value_of_json(arg(1)?)?),
        "引用" => Expr::Ref(arg(1)?.as_str().ok_or("引用: 地址应为字符串")?.to_string()),
        "加" => Expr::Add(bx(1)?, bx(2)?),
        "减" => Expr::Sub(bx(1)?, bx(2)?),
        "乘" => Expr::Mul(bx(1)?, bx(2)?),
        "除" => Expr::Div(bx(1)?, bx(2)?),
        "开方" => Expr::Sqrt(bx(1)?),
        "等于" => Expr::Eq(bx(1)?, bx(2)?),
        "小于" => Expr::Lt(bx(1)?, bx(2)?),
        "小于等于" => Expr::Le(bx(1)?, bx(2)?),
        "大于等于" => Expr::Ge(bx(1)?, bx(2)?),
        "大于" => Expr::Gt(bx(1)?, bx(2)?),
        "且" => Expr::And(bx(1)?, bx(2)?),
        "非" => Expr::Not(bx(1)?),
        "若" => Expr::If(bx(1)?, bx(2)?, bx(3)?),
        other => return Err(format!("未知标签: {other}（拒绝加载）")),
    })
}

/// 包（加载单位）
#[derive(Clone, Debug, Deserialize)]
pub struct Pack {
    #[serde(rename = "标准")] pub std_version: String,
    #[serde(rename = "包")] pub ns: String,
    #[serde(rename = "版本")] pub version: String,
    #[serde(rename = "三问")] pub gate: Gate,
    #[serde(rename = "模块", default)] pub modules: Vec<Module>,
    #[serde(rename = "关系", default)] pub relations: Vec<Relation>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Gate {
    #[serde(rename = "可推翻")] pub refutable: String,
    #[serde(rename = "论域")] pub domain: String,
    #[serde(rename = "检验者")] pub checker: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Port {
    #[serde(rename = "名")] pub name: String,
    #[serde(rename = "类型")] pub ty: String,
    #[serde(rename = "默认", default)] pub default: Option<serde_json::Value>,
    /// 逐条可见性（§10·5；S7）：缺省＝随模块默认。`输出`表上写了也不参与判定。
    #[serde(rename = "可见性", default)] pub vis: Option<String>,
    ///界式（§3；§14·3；S9）：**输入**可挂 `"界"`＝布尔表达式；缺省＝无界（旧包行为不变）。
    #[serde(rename = "界", default)] pub bound: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Variable {
    #[serde(rename = "名")] pub name: String,
    #[serde(rename = "类型")] pub ty: String,
    #[serde(rename = "初值")] pub init: serde_json::Value,
    /// 逐条可见性（§10·5；S7）：覆盖模块默认。
    #[serde(rename = "可见性", default)] pub vis: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Formula {
    #[serde(rename = "名")] pub name: String,
    #[serde(rename = "目标")] pub target: String,
    #[serde(rename = "表达式")] pub expr: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Module {
    pub id: String,
    /// 模块默认可见性档（§10·5；S7）：缺省＝`真`。变量／输入逐条声明可覆盖。
    #[serde(rename = "可见性", default)] pub vis: Option<String>,
    /// 归属者名（`私` 档的可见对象；§10·5）：模块内含 `私` 地址时必填。
    #[serde(rename = "归属", default)] pub owner: Option<String>,
    #[serde(rename = "输入", default)] pub inputs: Vec<Port>,
    #[serde(rename = "输出", default)] pub outputs: Vec<Port>,
    #[serde(rename = "变量", default)] pub vars: Vec<Variable>,
    #[serde(rename = "公式", default)] pub formulas: Vec<Formula>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Relation {
    #[serde(rename = "类型")] pub kind: String,
    #[serde(rename = "从", default)] pub from: Option<String>,
    #[serde(rename = "到", default)] pub to: Option<String>,
    #[serde(rename = "父", default)] pub parent: Option<String>,
    #[serde(rename = "子", default)] pub child: Option<String>,
}

/// 输入集（《引擎·标准》§11）：外部对入口的写，按 tick 成帧——「对局脚本」就是它。
#[derive(Clone, Debug, Deserialize)]
pub struct InputScript {
    #[serde(rename = "标准")] pub std_version: String,
    #[serde(rename = "包")] pub ns: String,
    #[serde(rename = "输入", default)] pub frames: Vec<InputFrame>,
}

/// 输入集的一帧（tick ＋ 写入）；S8 起也用作**存档的「输入」**（《引擎·标准》§13·1）——故可序列化。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputFrame {
    #[serde(rename = "tick")] pub tick: u64,
    #[serde(rename = "写", default)] pub writes: std::collections::BTreeMap<String, serde_json::Value>,
}
