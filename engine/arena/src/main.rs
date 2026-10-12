//! kuyuan-arena —— 竞技场批跑（平衡表 v0）
//! 用法: kuyuan-arena --pack <包.json> --matches N [--seed0 S] [--p1 r|h] [--p2 r|h] [--max-ticks N] [--json <out.json>]

use std::process::exit;

use kuyuan_arena::{Cause, SideStats, run_match};

fn usage() -> ! {
    eprintln!(
        "用法: kuyuan-arena --pack <包.json> --matches N [--seed0 S] [--p1 r|h] [--p2 r|h] [--max-ticks N] [--json <out.json>]"
    );
    exit(2);
}

#[derive(Default)]
struct Agg {
    n: u64,
    win: [u64; 2],
    draw: u64,
    cause: [u64; 7],
    ticks_sum: u64,
    decided_ticks_sum: u64,
    decided: u64,
    s: [SideStats; 2],
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut pack: Option<String> = None;
    let mut matches: u64 = 100;
    let mut seed0: u64 = 1;
    let mut p1 = "r".to_string();
    let mut p2 = "r".to_string();
    let mut max_ticks: u64 = 48;
    let mut json_out: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--pack" if i + 1 < args.len() => {
                pack = Some(args[i + 1].clone());
                i += 2;
            }
            "--matches" if i + 1 < args.len() => {
                matches = args[i + 1].parse().unwrap_or(100);
                i += 2;
            }
            "--seed0" if i + 1 < args.len() => {
                seed0 = args[i + 1].parse().unwrap_or(1);
                i += 2;
            }
            "--p1" if i + 1 < args.len() => {
                p1 = args[i + 1].clone();
                i += 2;
            }
            "--p2" if i + 1 < args.len() => {
                p2 = args[i + 1].clone();
                i += 2;
            }
            "--max-ticks" if i + 1 < args.len() => {
                max_ticks = args[i + 1].parse().unwrap_or(48);
                i += 2;
            }
            "--json" if i + 1 < args.len() => {
                json_out = Some(args[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }
    let pack = pack.unwrap_or_else(|| usage());
    let text = std::fs::read_to_string(&pack).unwrap_or_else(|e| {
        eprintln!("读包失败 {pack}: {e}");
        exit(1);
    });

    let mut agg = Agg::default();
    for m in 0..matches {
        let seed = seed0 + m;
        let r = run_match(&text, seed, &p1, &p2, max_ticks).unwrap_or_else(|e| {
            eprintln!("第 {seed} 局失败: {e}");
            exit(1);
        });
        let o = &r.outcome;
        agg.n += 1;
        match o.result {
            1 => agg.win[0] += 1,
            2 => agg.win[1] += 1,
            3 => agg.draw += 1,
            _ => {}
        }
        agg.cause[o.cause.idx()] += 1;
        agg.ticks_sum += o.ticks;
        if o.result == 1 || o.result == 2 {
            agg.decided += 1;
            agg.decided_ticks_sum += o.ticks;
        }
        for k in 0..2usize {
            let src = if k == 0 { &o.p1 } else { &o.p2 };
            let dst = &mut agg.s[k];
            dst.moves += src.moves;
            dst.probes += src.probes;
            dst.bombs += src.bombs;
            dst.scans += src.scans;
            dst.warns += src.warns;
            dst.counters += src.counters;
            dst.hears += src.hears;
        }
    }

    let n = agg.n as f64;
    println!(
        "== 竞技场 · {p1} vs {p2} · {} 局（种子 {}–{}） ==",
        agg.n,
        seed0,
        seed0 + agg.n.saturating_sub(1)
    );
    println!(
        "结果：P1胜 {}（{:.1}%）· P2胜 {}（{:.1}%）· 平 {}（{:.1}%）",
        agg.win[0],
        pct(agg.win[0], n),
        agg.win[1],
        pct(agg.win[1], n),
        agg.draw,
        pct(agg.draw, n)
    );
    let avg = |sum: u64, d: u64| if d == 0 { 0.0 } else { sum as f64 / d as f64 };
    println!(
        "拍数：均值 {:.1}（分出胜负 {:.1} · 平局 {:.1}）",
        avg(agg.ticks_sum, agg.n),
        avg(agg.decided_ticks_sum, agg.decided),
        avg(agg.ticks_sum - agg.decided_ticks_sum, agg.n - agg.decided)
    );
    let mut cs: Vec<String> = Vec::new();
    for c in Cause::ALL {
        cs.push(format!("{} {:.1}%", c.label(), pct(agg.cause[c.idx()], n)));
    }
    println!("终局方式：{}", cs.join(" · "));
    println!(
        "情报（每局 · P1/P2）：发探 {:.2}/{:.2} · 扫描命中 {:.2}/{:.2} · 被警 {:.2}/{:.2} · 反炮 {:.2}/{:.2} · 发弹 {:.2}/{:.2} · 移动 {:.2}/{:.2}",
        avg(agg.s[0].probes, agg.n),
        avg(agg.s[1].probes, agg.n),
        avg(agg.s[0].scans, agg.n),
        avg(agg.s[1].scans, agg.n),
        avg(agg.s[0].warns, agg.n),
        avg(agg.s[1].warns, agg.n),
        avg(agg.s[0].counters, agg.n),
        avg(agg.s[1].counters, agg.n),
        avg(agg.s[0].bombs, agg.n),
        avg(agg.s[1].bombs, agg.n),
        avg(agg.s[0].moves, agg.n),
        avg(agg.s[1].moves, agg.n)
    );

    if let Some(path) = json_out {
        let mut causes = serde_json::Map::new();
        for c in Cause::ALL {
            causes.insert(c.label().to_string(), serde_json::json!(agg.cause[c.idx()]));
        }
        let side = |s: &SideStats| {
            serde_json::json!({
                "moves": s.moves, "probes": s.probes, "bombs": s.bombs,
                "scans": s.scans, "warns": s.warns, "counters": s.counters, "hears": s.hears,
            })
        };
        let doc = serde_json::json!({
            "p1": p1, "p2": p2, "matches": agg.n, "seed0": seed0,
            "win": {"p1": agg.win[0], "p2": agg.win[1]}, "draw": agg.draw,
            "avg_ticks": avg(agg.ticks_sum, agg.n),
            "causes": causes,
            "stats": {"p1": side(&agg.s[0]), "p2": side(&agg.s[1])},
        });
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap_or_default())
            .unwrap_or_else(|e| {
                eprintln!("写 JSON 失败 {path}: {e}");
                exit(1);
            });
        println!("JSON 已写: {path}");
    }
}

fn pct(x: u64, n: f64) -> f64 {
    if n == 0.0 { 0.0 } else { x as f64 * 100.0 / n }
}