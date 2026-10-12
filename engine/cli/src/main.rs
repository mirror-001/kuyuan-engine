//! 库元引擎 · cli —— 跑包：`kuyuan-cli run <包.json> [--ticks N] [--inputs <输入集.json>] [--view <视角名>] [--max-ticks N]`。
//! `--inputs` ＝《引擎·标准》§11 的输入集（对局脚本）：按 tick 给入口写值。
//! `--view` ＝ §10·5 的界：按视角投影事件流与快照（缺省＝`裁判`＝全见，故不带 `--view` 时输出逐字节不变）。
use std::collections::BTreeMap;
use std::process::exit;

use kuyuan_core::{value_of_json, Value};

fn usage() -> ! {
    eprintln!(
        "用法: kuyuan-cli run <包.json> [--ticks N] [--inputs <输入集.json>] [--view <视角名>] [--max-ticks N]"
    );
    exit(2);
}

fn read_or_die(path: &str, what: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{what}: {e}");
            exit(1);
        }
    }
}

///打印（按视角过滤后的）拒绝日志——加载期与运行期失败共用（§10·5）。
fn print_rejects(world: &kuyuan_driver::World, log: &kuyuan_log::EventLog, view: &str) {
    for ev in kuyuan_view::events(world, &log.events, view)
        .iter()
        .filter(|e| e.kind == kuyuan_log::KIND_REJECT)
    {
        eprintln!("拒绝日志: {}", serde_json::to_string(ev).unwrap_or_default());
    }
}


fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] != "run" {
        usage();
    }
    let path = &args[2];
    let mut ticks: u64 = 2;
    let mut inputs_path: Option<String> = None;
    let mut view_arg: Option<String> = None;
    //时间界（§14·5；S9）：一次运行可声明**拍上限**——超限＝拒绝推进（先记后 Err）。
    let mut max_ticks: Option<u64> = None;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--ticks" if i + 1 < args.len() => {
                ticks = args[i + 1].parse().unwrap_or(2);
                i += 2;
            }
            "--inputs" if i + 1 < args.len() => {
                inputs_path = Some(args[i + 1].clone());
                i += 2;
            }
            // 界（§10·5；S7）：缺省＝裁判＝全见。
            "--view" if i + 1 < args.len() => {
                view_arg = Some(args[i + 1].clone());
                i += 2;
            }
            //时间界（§14·5；S9）：声明拍上限——缺省＝无界（旧行为不变）。
            "--max-ticks" if i + 1 < args.len() => {
                max_ticks = Some(args[i + 1].parse().unwrap_or(u64::MAX));
                i += 2;
            }

            _ => i += 1,
        }
    }

    let text = read_or_die(path, "读包失败");
    let pack: kuyuan_core::Pack = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("解析包失败: {e}");
            exit(1);
        }
    };
    // 事件日志＝唯一事实源（S4）：加载期被拒也进它（§7.2），故先建后加载。
    let mut log = kuyuan_log::EventLog::new();
    let mut world = match kuyuan_driver::load_logged(&pack, &mut log) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("加载被拒: {e}");
            for ev in &log.events {
                eprintln!("拒绝日志: {}", serde_json::to_string(ev).unwrap_or_default());
            }
            exit(1);
        }
    };

    // 输入集（§11）：脚本 → 每 tick 一帧
    let mut frames: BTreeMap<u64, BTreeMap<String, Value>> = BTreeMap::new();
    if let Some(p) = &inputs_path {
        let t = read_or_die(p, "读输入集失败");
        let script: kuyuan_core::InputScript = match serde_json::from_str(&t) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("解析输入集失败: {e}");
                exit(1);
            }
        };
        if !script.std_version.starts_with("库元/引擎/") {
            eprintln!("输入集标准版本不认识: {}", script.std_version);
            exit(1);
        }
        if script.ns != pack.ns {
            eprintln!("输入集包名不符: {} ≠ {}", script.ns, pack.ns);
            exit(1);
        }
        for f in &script.frames {
            let mut m: BTreeMap<String, Value> = BTreeMap::new();
            for (k, v) in &f.writes {
                match value_of_json(v) {
                    Ok(val) => {
                        m.insert(k.clone(), val);
                    }
                    Err(e) => {
                        eprintln!("输入值不合法（tick {} · {}）: {e}", f.tick, k);
                        exit(1);
                    }
                }
            }
            frames.insert(f.tick, m);
        }
    }

    let empty: BTreeMap<String, Value> = BTreeMap::new();
    let explicit_view = view_arg.is_some();
    let view = view_arg.unwrap_or_else(|| kuyuan_core::JUDGE.to_string());
    for t in 1..=ticks {
        //时间界（§14·5；S9）：声明了拍上限则推进前过判定——超限＝先记「拒绝」再拒（§7.2）。
        if let Some(cap) = max_ticks {
            if let Err(e) = kuyuan_guard::check_limit(t, cap, &mut log) {
                eprintln!("tick {t}失败: {e}");
                print_rejects(&world, &log, &view);
                exit(1);
            }
        }

        let frame = frames.get(&t).unwrap_or(&empty);
        if let Err(e) = world.tick_in(t, frame, &mut log) {
            eprintln!("tick {t} 失败: {e}");
            //拒绝日志也按视角过滤（§10·5）——目标＝地址且该地址不可见者不出现。
        print_rejects(&world, &log, &view);
        exit(1);
        }
    }

    // 界（S7）：投影只作用于**这份输出**——世界与原始日志不动（§10·5）。
    let evs = kuyuan_view::events(&world, &log.events, &view);
    if explicit_view {
        println!(
            "== 视角: {view}（可见 {} 地址 · 事件 {} 条） ==",
            kuyuan_view::visible_count(&world, &view),
            evs.len()
        );
    }
    println!("== 事件流（{} 条） ==", evs.len());
    for e in &evs {
        println!("{}", serde_json::to_string(e).unwrap_or_default());
    }
    println!("== 快照 ==");
    println!("{}", kuyuan_view::snapshot_json(&world, &view));
}
