//! 库元引擎 · eval —— 求值器（最小 AST · 同步单遍的“读”）。
use std::collections::BTreeMap;

use kuyuan_core::{Expr, Value};

pub type State = BTreeMap<String, Value>;

pub fn eval(e: &Expr, s: &State) -> Result<Value, String> {
    match e {
        Expr::Lit(v) => Ok(v.clone()),
        Expr::Ref(a) => s.get(a).cloned().ok_or_else(|| format!("引用未定义: {a}")),
        Expr::Add(a, b) => bin(eval(a, s)?, eval(b, s)?, "加", i64::checked_add, |x, y| x + y),
        Expr::Sub(a, b) => bin(eval(a, s)?, eval(b, s)?, "减", i64::checked_sub, |x, y| x - y),
        Expr::Mul(a, b) => bin(eval(a, s)?, eval(b, s)?, "乘", i64::checked_mul, |x, y| x * y),
        Expr::Div(a, b) => {
            let (x, y) = (eval(a, s)?, eval(b, s)?);
            match (&x, &y) {
                (Value::Int(_), Value::Int(0)) => Err("整数除零（拒绝）".into()),
                (Value::Int(i), Value::Int(j)) => i
                    .checked_div(*j)
                    .map(Value::Int)
                    .ok_or_else(|| "整数除法溢出（拒绝）".to_string()),
                _ => {
                    if !x.is_num() || !y.is_num() {
                        return Err(format!("除: 类型错误（{} , {}）", x.type_name(), y.type_name()));
                    }
                    Ok(Value::Real(x.as_f64().unwrap() / y.as_f64().unwrap()))
                }
            }
        }
        Expr::Sqrt(a) => {
            let v = eval(a, s)?;
            v.as_f64()
                .map(|f| Value::Real(f.sqrt()))
                .ok_or_else(|| format!("开方: 类型错误（{}）", v.type_name()))
        }
        Expr::Eq(a, b) => {
            let (x, y) = (eval(a, s)?, eval(b, s)?);
            let eq = match (&x, &y) {
                (Value::Int(i), Value::Int(j)) => i == j,
                (Value::Bool(i), Value::Bool(j)) => i == j,
                (Value::Text(i), Value::Text(j)) => i == j,
                _ if x.is_num() && y.is_num() => x.as_f64().unwrap() == y.as_f64().unwrap(),
                _ => return Err(format!("等于: 类型错误（{} , {}）", x.type_name(), y.type_name())),
            };
            Ok(Value::Bool(eq))
        }
        Expr::Lt(a, b) => cmp(eval(a, s)?, eval(b, s)?, "小于", |x, y| x < y),
        Expr::Le(a, b) => cmp(eval(a, s)?, eval(b, s)?, "小于等于", |x, y| x <= y),
        Expr::Ge(a, b) => cmp(eval(a, s)?, eval(b, s)?, "大于等于", |x, y| x >= y),
        Expr::Gt(a, b) => cmp(eval(a, s)?, eval(b, s)?, "大于", |x, y| x > y),
        Expr::And(a, b) => match eval(a, s)? {
            Value::Bool(false) => Ok(Value::Bool(false)),
            Value::Bool(true) => match eval(b, s)? {
                Value::Bool(v) => Ok(Value::Bool(v)),
                other => Err(format!("且: 右值非布尔（{}）", other.type_name())),
            },
            other => Err(format!("且: 左值非布尔（{}）", other.type_name())),
        },
        Expr::Not(a) => match eval(a, s)? {
            Value::Bool(v) => Ok(Value::Bool(!v)),
            other => Err(format!("非: 类型错误（{}）", other.type_name())),
        },
        Expr::If(c, t, f) => match eval(c, s)? {
            Value::Bool(true) => eval(t, s),
            Value::Bool(false) => eval(f, s),
            other => Err(format!("若: 条件非布尔（{}）", other.type_name())),
        },
    }
}

fn bin(a: Value, b: Value, op: &str, fi: fn(i64, i64) -> Option<i64>, fr: fn(f64, f64) -> f64) -> Result<Value, String> {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => fi(*x, *y)
            .map(Value::Int)
            .ok_or_else(|| format!("整数{op}溢出（checked，拒绝）")),
        _ => {
            if !a.is_num() || !b.is_num() {
                return Err(format!("{op}: 类型错误（{} , {}）", a.type_name(), b.type_name()));
            }
            Ok(Value::Real(fr(a.as_f64().unwrap(), b.as_f64().unwrap())))
        }
    }
}

fn cmp(a: Value, b: Value, op: &str, f: fn(f64, f64) -> bool) -> Result<Value, String> {
    if !a.is_num() || !b.is_num() {
        return Err(format!("{op}: 需要数值（{} , {}）", a.type_name(), b.type_name()));
    }
    Ok(Value::Bool(f(a.as_f64().unwrap(), b.as_f64().unwrap())))
}

// ---------- 加载期类型推断（§12 表 19；裁 #30 收口） ----------

/// 该类型名是不是数值（整数／实数）。
fn is_num_ty(t: &str) -> bool {
    t == "整数" || t == "实数"
}

/// 表达式的**类型推断**（照运行期语义 §10·7 静态推一遍）：`Ok(类型名)` 或 `Err(理由)`。
/// 契约：`整数⊂实数`（§3）；`除`：整数／整数＝整数（checked），否则实数；`开方`＝实数；
/// 比较＝布尔；`且`／`非`＝布尔；`若` 两支须相容（整数／实数混用即实数）。
/// 调用方（`build`）据此在**加载期**拒绝——「加载期能判的不留给运行期」。
pub fn type_of(e: &Expr, types: &BTreeMap<String, String>) -> Result<String, String> {
    let both_num = |op: &str, x: String, y: String| -> Result<String, String> {
        if !is_num_ty(&x) || !is_num_ty(&y) {
            return Err(format!("`{op}` 需数值，得（{x} , {y}）"));
        }
        Ok(if x == "整数" && y == "整数" { "整数" } else { "实数" }.to_string())
    };
    match e {
        Expr::Lit(v) => Ok(v.type_name().to_string()),
        Expr::Ref(a) => types.get(a).cloned().ok_or_else(|| format!("引用不在场: {a}")),
        Expr::Add(a, b) => both_num("加", type_of(a, types)?, type_of(b, types)?),
        Expr::Sub(a, b) => both_num("减", type_of(a, types)?, type_of(b, types)?),
        Expr::Mul(a, b) => both_num("乘", type_of(a, types)?, type_of(b, types)?),
        Expr::Div(a, b) => both_num("除", type_of(a, types)?, type_of(b, types)?),
        Expr::Sqrt(a) => {
            let t = type_of(a, types)?;
            if !is_num_ty(&t) {
                return Err(format!("`开方` 需数值，得（{t}）"));
            }
            Ok("实数".to_string())
        }
        Expr::Eq(a, b) => {
            let (x, y) = (type_of(a, types)?, type_of(b, types)?);
            if (is_num_ty(&x) && is_num_ty(&y)) || x == y {
                Ok("布尔".to_string())
            } else {
                Err(format!("`等于` 类型不相容（{x} , {y}）"))
            }
        }
        Expr::Lt(a, b) => both_num("小于", type_of(a, types)?, type_of(b, types)?).map(|_| "布尔".into()),
        Expr::Le(a, b) => {
            both_num("小于等于", type_of(a, types)?, type_of(b, types)?).map(|_| "布尔".into())
        }
        Expr::Ge(a, b) => {
            both_num("大于等于", type_of(a, types)?, type_of(b, types)?).map(|_| "布尔".into())
        }
        Expr::Gt(a, b) => {
            both_num("大于", type_of(a, types)?, type_of(b, types)?).map(|_| "布尔".into())
        }
        Expr::And(a, b) => {
            let (x, y) = (type_of(a, types)?, type_of(b, types)?);
            if x == "布尔" && y == "布尔" {
                Ok("布尔".to_string())
            } else {
                Err(format!("`且` 需布尔，得（{x} , {y}）"))
            }
        }
        Expr::Not(a) => {
            let t = type_of(a, types)?;
            if t == "布尔" {
                Ok("布尔".to_string())
            } else {
                Err(format!("`非` 需布尔，得（{t}）"))
            }
        }
        Expr::If(c, t, f) => {
            let (ct, tt, ft) = (type_of(c, types)?, type_of(t, types)?, type_of(f, types)?);
            if ct != "布尔" {
                return Err(format!("`若` 条件需布尔，得（{ct}）"));
            }
            if tt == ft {
                Ok(tt)
            } else if is_num_ty(&tt) && is_num_ty(&ft) {
                Ok("实数".to_string())
            } else {
                Err(format!("`若` 两支类型不相容（{tt} , {ft}）"))
            }
        }
    }
}

// ---------- S2b：编译后的 AST（引用 → 槽位） ----------

/// 编译后的表达式：引用已解析为槽位索引，求值不再做字符串查找。
#[derive(Clone, Debug)]
pub enum CExpr {
    Lit(Value),
    Slot(usize),
    /// 地址不在场：`build` 在加载期据此拒绝（S3 起）；运行期兜底报「引用未定义」。
    Missing(String),
    Add(Box<CExpr>, Box<CExpr>),
    Sub(Box<CExpr>, Box<CExpr>),
    Mul(Box<CExpr>, Box<CExpr>),
    Div(Box<CExpr>, Box<CExpr>),
    Sqrt(Box<CExpr>),
    Eq(Box<CExpr>, Box<CExpr>),
    Lt(Box<CExpr>, Box<CExpr>),
    Le(Box<CExpr>, Box<CExpr>),
    Ge(Box<CExpr>, Box<CExpr>),
    Gt(Box<CExpr>, Box<CExpr>),
    And(Box<CExpr>, Box<CExpr>),
    Not(Box<CExpr>),
    If(Box<CExpr>, Box<CExpr>, Box<CExpr>),
}

/// 编译：包格式 AST（地址）→ 运行期 AST（槽位索引）。地址不在场记为 `Missing`，
/// 由 `build` 在**加载期**拒绝（S3 起）；此处保留 `Missing` 作为运行期的兜底。
pub fn compile(e: &Expr, index: &BTreeMap<String, usize>) -> CExpr {
    let b = |x: &Expr| Box::new(compile(x, index));
    match e {
        Expr::Lit(v) => CExpr::Lit(v.clone()),
        Expr::Ref(a) => match index.get(a) {
            Some(i) => CExpr::Slot(*i),
            None => CExpr::Missing(a.clone()),
        },
        Expr::Add(a, c) => CExpr::Add(b(a), b(c)),
        Expr::Sub(a, c) => CExpr::Sub(b(a), b(c)),
        Expr::Mul(a, c) => CExpr::Mul(b(a), b(c)),
        Expr::Div(a, c) => CExpr::Div(b(a), b(c)),
        Expr::Sqrt(a) => CExpr::Sqrt(b(a)),
        Expr::Eq(a, c) => CExpr::Eq(b(a), b(c)),
        Expr::Lt(a, c) => CExpr::Lt(b(a), b(c)),
        Expr::Le(a, c) => CExpr::Le(b(a), b(c)),
        Expr::Ge(a, c) => CExpr::Ge(b(a), b(c)),
        Expr::Gt(a, c) => CExpr::Gt(b(a), b(c)),
        Expr::And(a, c) => CExpr::And(b(a), b(c)),
        Expr::Not(a) => CExpr::Not(b(a)),
        Expr::If(a, c, d) => CExpr::If(b(a), b(c), b(d)),
    }
}

/// 未解析引用的地址表（加载期门禁用；《引擎·标准》§1 名字是地址）。
pub fn unresolved_addrs(e: &CExpr) -> Vec<String> {
    fn walk(e: &CExpr, out: &mut Vec<String>) {
        match e {
            CExpr::Missing(a) => out.push(a.clone()),
            CExpr::Lit(_) | CExpr::Slot(_) => {}
            CExpr::Add(a, b)
            | CExpr::Sub(a, b)
            | CExpr::Mul(a, b)
            | CExpr::Div(a, b)
            | CExpr::Eq(a, b)
            | CExpr::Lt(a, b)
            | CExpr::Le(a, b)
            | CExpr::Ge(a, b)
            | CExpr::Gt(a, b)
            | CExpr::And(a, b) => {
                walk(a, out);
                walk(b, out);
            }
            CExpr::Sqrt(a) | CExpr::Not(a) => walk(a, out),
            CExpr::If(a, b, c) => {
                walk(a, out);
                walk(b, out);
                walk(c, out);
            }
        }
    }
    let mut out: Vec<String> = vec![];
    walk(e, &mut out);
    out
}

/// 槽位求值：与 `eval` 逐条同义，只是引用读 `slots[i]`。
pub fn eval_c(e: &CExpr, slots: &[Value]) -> Result<Value, String> {
    match e {
        CExpr::Lit(v) => Ok(v.clone()),
        CExpr::Slot(i) => slots.get(*i).cloned().ok_or_else(|| format!("槽位越界: {i}")),
        CExpr::Missing(a) => Err(format!("引用未定义: {a}")),
        CExpr::Add(a, b) => bin(eval_c(a, slots)?, eval_c(b, slots)?, "加", i64::checked_add, |x, y| x + y),
        CExpr::Sub(a, b) => bin(eval_c(a, slots)?, eval_c(b, slots)?, "减", i64::checked_sub, |x, y| x - y),
        CExpr::Mul(a, b) => bin(eval_c(a, slots)?, eval_c(b, slots)?, "乘", i64::checked_mul, |x, y| x * y),
        CExpr::Div(a, b) => {
            let (x, y) = (eval_c(a, slots)?, eval_c(b, slots)?);
            match (&x, &y) {
                (Value::Int(_), Value::Int(0)) => Err("整数除零（拒绝）".into()),
                (Value::Int(i), Value::Int(j)) => i
                    .checked_div(*j)
                    .map(Value::Int)
                    .ok_or_else(|| "整数除法溢出（拒绝）".to_string()),
                _ => {
                    if !x.is_num() || !y.is_num() {
                        return Err(format!("除: 类型错误（{} , {}）", x.type_name(), y.type_name()));
                    }
                    Ok(Value::Real(x.as_f64().unwrap() / y.as_f64().unwrap()))
                }
            }
        }
        CExpr::Sqrt(a) => {
            let v = eval_c(a, slots)?;
            v.as_f64()
                .map(|f| Value::Real(f.sqrt()))
                .ok_or_else(|| format!("开方: 类型错误（{}）", v.type_name()))
        }
        CExpr::Eq(a, b) => {
            let (x, y) = (eval_c(a, slots)?, eval_c(b, slots)?);
            let eq = match (&x, &y) {
                (Value::Int(i), Value::Int(j)) => i == j,
                (Value::Bool(i), Value::Bool(j)) => i == j,
                (Value::Text(i), Value::Text(j)) => i == j,
                _ if x.is_num() && y.is_num() => x.as_f64().unwrap() == y.as_f64().unwrap(),
                _ => return Err(format!("等于: 类型错误（{} , {}）", x.type_name(), y.type_name())),
            };
            Ok(Value::Bool(eq))
        }
        CExpr::Lt(a, b) => cmp(eval_c(a, slots)?, eval_c(b, slots)?, "小于", |x, y| x < y),
        CExpr::Le(a, b) => cmp(eval_c(a, slots)?, eval_c(b, slots)?, "小于等于", |x, y| x <= y),
        CExpr::Ge(a, b) => cmp(eval_c(a, slots)?, eval_c(b, slots)?, "大于等于", |x, y| x >= y),
        CExpr::Gt(a, b) => cmp(eval_c(a, slots)?, eval_c(b, slots)?, "大于", |x, y| x > y),
        CExpr::And(a, b) => match eval_c(a, slots)? {
            Value::Bool(false) => Ok(Value::Bool(false)),
            Value::Bool(true) => match eval_c(b, slots)? {
                Value::Bool(v) => Ok(Value::Bool(v)),
                other => Err(format!("且: 右值非布尔（{}）", other.type_name())),
            },
            other => Err(format!("且: 左值非布尔（{}）", other.type_name())),
        },
        CExpr::Not(a) => match eval_c(a, slots)? {
            Value::Bool(v) => Ok(Value::Bool(!v)),
            other => Err(format!("非: 类型错误（{}）", other.type_name())),
        },
        CExpr::If(c, t, f) => match eval_c(c, slots)? {
            Value::Bool(true) => eval_c(t, slots),
            Value::Bool(false) => eval_c(f, slots),
            other => Err(format!("若: 条件非布尔（{}）", other.type_name())),
        },
    }
}
