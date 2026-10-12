//! 库元引擎 · store —— 存档 / 回放 / 回滚（构成四样之「**投影**」支；`view` 之后第二件）。
//! 契约：《引擎·标准》§13（v0.3.11；S8）。
//!
//! 一句话：**引用式 ＋ 重算式**——存档**不自足**，只记"重演配方"：
//! **Pack 指纹 ＋ 输入集 ＋ 拍数 ＋ 封条**；不含事件流（重演自会重建）、不含视角名（§10·5）。
//!
//! 三道校验（任一不过＝**拒绝**，不猜、不降级）：标准版本 → 指纹 → 封条。
//! 对账（裁 #37 的解药）：世界带「拍」（#42＝乙）· 账本带 `last_tick` —— 两者必须相等。
//! 回滚（裁 #35）：**不改不截断旧账，另起分支**——旧账与旧世界一个字节不动。

use std::collections::BTreeMap;

use kuyuan_core::{value_of_json, InputFrame, Pack, Value};
use kuyuan_driver::{load, World};
use kuyuan_log::EventLog;
use serde::{Deserialize, Serialize};

/// 存档标准版本（§13·1）。
pub const STD: &str = "库元/引擎/存档/v0";

/// 存档（§13·1）：引用式＋重算式——"重演配方"。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Archive {
    #[serde(rename = "标准")] pub std_version: String,
    #[serde(rename = "包")] pub ns: String,
    #[serde(rename = "拍")] pub tick: u64,
    #[serde(rename = "指纹")] pub fingerprint: String,
    #[serde(rename = "输入")] pub frames: Vec<InputFrame>,
    #[serde(rename = "封条")] pub seal: String,
}

/// 分支信息（§13·4）：回滚＝另起分支——记下"从哪条存档的哪一拍分出来"。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    #[serde(rename = "源指纹")] pub from_fingerprint: String,
    #[serde(rename = "源拍")] pub from_tick: u64,
}

/// 指纹（§13·1）：FNV-1a 64 位，跨端逐位可算；`fnv1a64:` ＋ 16 位小写十六进制。
/// 用于**识别与对账**，非防篡改（不作密码学散列）。
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}")
}

/// 封条（§13·1）：指纹（事件流文本 ＋ `\n--\n` ＋ 快照文本）。
/// 事件流文本＝每条事件单行 JSON、按序以 LF 相接；快照文本＝`snapshot_json`。
/// **「重演与原始运行逐字节相等」的落点。**
pub fn seal_of(world: &World, log: &EventLog) -> String {
    let events = log
        .events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let snapshot = world.snapshot_json();
    fingerprint(format!("{events}\n--\n{snapshot}").as_bytes())
}

/// 对账（§13·3；裁 #37 的解药）：世界「拍」＝ 账本 `last_tick`，否则拒绝。
pub fn reconcile(world: &World, log: &EventLog) -> Result<(), String> {
    if world.tick == log.last_tick {
        return Ok(());
    }
    Err(format!(
        "世界与账本不一致: 世界在第 {} 拍、账本在第 {} 拍 —— 拒绝对账",
        world.tick, log.last_tick
    ))
}

/// 重演（内部）：从包重跑第 1..=upto 拍；任一拍被拒＝整体拒绝（不静默）。
/// 同一拍多帧＝本批不支持（拒——多试输入集属后置，§13·5）。
fn rebuild(pack: &Pack, frames: &[InputFrame], upto: u64) -> Result<(World, EventLog), String> {
    let mut by_tick: BTreeMap<u64, &InputFrame> = BTreeMap::new();
    for f in frames {
        if by_tick.insert(f.tick, f).is_some() {
            return Err(format!("输入集同一拍多帧（第 {} 拍）—— 本批不支持（拒绝重演）", f.tick));
        }
    }
    let mut world = load(pack).map_err(|e| format!("加载被拒: {e}"))?;
    let mut log = EventLog::new();
    for t in 1..=upto {
        let mut writes: BTreeMap<String, Value> = BTreeMap::new();
        if let Some(f) = by_tick.get(&t) {
            for (addr, v) in &f.writes {
                writes.insert(
                    addr.clone(),
                    value_of_json(v).map_err(|e| format!("输入值不合法（第 {t} 拍 · {addr}）: {e}"))?,
                );
            }
        }
        world
            .tick_in(t, &writes, &mut log)
            .map_err(|e| format!("重演被拒（第 {t} 拍）: {e}"))?;
    }
    Ok((world, log))
}

/// 存档（§13·1）：**存档即验**——落盘前先按"指纹＋输入集＋拍数"原地重演，
/// 与给定世界／账本**封条相等**才出存档；不等＝**拒绝存档**（拒优于猜）。
pub fn save(
    pack_bytes: &[u8],
    frames: &[InputFrame],
    world: &World,
    log: &EventLog,
) -> Result<Archive, String> {
    reconcile(world, log)?;
    let pack: Pack = serde_json::from_slice(pack_bytes).map_err(|e| format!("包解析失败: {e}"))?;
    let seal = seal_of(world, log);
    let (w, l) = rebuild(&pack, frames, world.tick)?;
    if seal_of(&w, &l) != seal {
        return Err(format!(
            "存档不自洽: 按「指纹＋输入集＋拍数」重演（第 {} 拍），与给定世界／账本封条不符 —— 拒绝存档",
            world.tick
        ));
    }
    Ok(Archive {
        std_version: STD.to_string(),
        ns: pack.ns.clone(),
        tick: world.tick,
        fingerprint: fingerprint(pack_bytes),
        frames: frames.to_vec(),
        seal,
    })
}

/// 回放（§13·2）：三道校验（标准版本 → 指纹 → 封条），任一不过＝**拒绝回放**。
pub fn replay(pack_bytes: &[u8], archive: &Archive) -> Result<(World, EventLog), String> {
    if archive.std_version != STD {
        return Err(format!("存档标准版本不认识: {}", archive.std_version));
    }
    let got = fingerprint(pack_bytes);
    if got != archive.fingerprint {
        return Err(format!(
            "指纹不符: 存档记「{}」，实得「{got}」—— 拒绝回放（Pack 不在场或已变）",
            archive.fingerprint
        ));
    }
    let pack: Pack = serde_json::from_slice(pack_bytes).map_err(|e| format!("包解析失败: {e}"))?;
    if pack.ns != archive.ns {
        return Err(format!("包名不符: 存档是「{}」，实得「{}」—— 拒绝回放", archive.ns, pack.ns));
    }
    let (world, log) = rebuild(&pack, &archive.frames, archive.tick)?;
    let got = seal_of(&world, &log);
    if got != archive.seal {
        return Err(format!(
            "封条不符: 重演结果「{got}」≠ 存档「{}」—— 拒绝回放（重演与原始运行不相等）",
            archive.seal
        ));
    }
    Ok((world, log))
}

/// 回滚（§13·4；裁 #35）：**分支 ＋ 重建**——不改、不截断既有账本；
/// 世界′＝重演到存档「拍」；**新账只记此后**（`序` 自 0，`last_tick` 起手＝源拍）。
/// 既有账本与既有世界**一个字节不动**。
pub fn branch(pack_bytes: &[u8], archive: &Archive) -> Result<(World, EventLog, Branch), String> {
    let (world, _replayed) = replay(pack_bytes, archive)?;
    let log = EventLog::fork(archive.tick);
    Ok((
        world,
        log,
        Branch { from_fingerprint: archive.fingerprint.clone(), from_tick: archive.tick },
    ))
}