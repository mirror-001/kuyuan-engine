//! 库元引擎 · play —— 网页对局壳（门槛件批②：会话核 ＋ 极简 HTTP ＋ 静态页）。
//!
//! 用法：`kuyuan-play <包.json> [--port N] [--seed N]`
//! 起服务后，手机浏览器开 `http://127.0.0.1:<port>/` 即玩。
//! 存档：$KUYUAN_SAVE_DIR 或 $HOME/.kuyuan（单槽 · 存/读按钮直连；store 引用式存档）。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use kuyuan_play::session::{Opponent, Session};

/// 静态页（随壳内嵌；改页即改壳）。
const PAGE: &str = include_str!("page.html");

const HTML: &str = "text/html; charset=utf-8";
const JSON: &str = "application/json; charset=utf-8";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: kuyuan-play <包.json> [--port N] [--seed N]");
        std::process::exit(2);
    }
    let pack_path = args[1].clone();
    let mut port: u16 = 8092;
    let mut seed: u64 = seed_now();
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(8092);
                i += 2;
            }
            "--seed" if i + 1 < args.len() => {
                seed = args[i + 1].parse().unwrap_or(seed);
                i += 2;
            }
            _ => i += 1,
        }
    }

    let pack_text = match std::fs::read_to_string(&pack_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("读包失败: {e}");
            std::process::exit(1);
        }
    };
    let mut ss = match Session::new(&pack_text, seed) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("开局失败: {e}");
            std::process::exit(1);
        }
    };

    println!("== 32×32 · 网页对局（最小版）==");
    println!("包: {pack_path}（玩家1＝你 · 玩家2＝对手：听风／猎手／随机／自定义） seed={seed}");
    println!("开: http://127.0.0.1:{port}/ （手机浏览器）");
    println!("存档: {}", save_file().display());

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("绑定失败: {e}");
            std::process::exit(1);
        }
    };

    let mut rng_seed = seed;
    for stream in listener.incoming() {
        if let Ok(mut s) = stream {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let _ = s.set_write_timeout(Some(std::time::Duration::from_secs(5)));
            handle(&mut s, &mut ss, &mut rng_seed, &pack_text);
        }
    }
}

fn handle(stream: &mut TcpStream, ss: &mut Session, seed: &mut u64, pack_text: &str) {
    let (method, path, query, body) = match read_request(stream) {
        Ok(r) => r,
        Err(_) => {
            let _ = respond(stream, 400, JSON, "{\"error\":\"请求不完整\"}");
            return;
        }
    };
    println!("{method} {path}");

    let out: (u16, &str, String) = match (method.as_str(), path.as_str()) {
        ("GET", "/") => (200, HTML, PAGE.to_string()),
        ("GET", "/api/state") => (200, JSON, ss.state_json().to_string()),
        ("GET", "/api/replay") => {
            let t = query_value(&query, "t").unwrap_or(u64::MAX);
            match ss.replay_json(t) {
                Ok(v) => (200, JSON, v.to_string()),
                Err(e) => (400, JSON, serde_json::json!({ "error": e }).to_string()),
            }
        }
        ("POST", "/api/act") => match parse_act(&body).and_then(|(a, d, r)| ss.act(a, d, r)) {
            Ok(()) => (200, JSON, ss.state_json().to_string()),
            Err(e) => (400, JSON, serde_json::json!({ "error": e }).to_string()),
        },
        ("POST", "/api/new") => {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1;
            // 对手可在开局时选定（body 为空＝默认「听风」）。
            let spec = if body.trim().is_empty() {
                Ok(Opponent::default())
            } else {
                serde_json::from_str::<serde_json::Value>(&body)
                    .map_err(|e| format!("对手配置不是 JSON: {e}"))
                    .and_then(|v| Opponent::request(&v))
            };
            let r = spec
                .and_then(|sp| ss.restart(*seed).map(|_| sp))
                .and_then(|sp| ss.set_opponent(sp));
            match r {
                Ok(()) => (200, JSON, ss.state_json().to_string()),
                Err(e) => (400, JSON, serde_json::json!({ "error": e }).to_string()),
            }
        }
        ("POST", "/api/save") => match ss.save_json() {
            Ok(v) => {
                let p = save_file();
                let w = std::fs::create_dir_all(p.parent().unwrap_or(std::path::Path::new(".")))
                    .and_then(|_| std::fs::write(&p, v.to_string()));
                match w {
                    Ok(()) => (
                        200,
                        JSON,
                        serde_json::json!({"ok": true, "tick": ss.tick(), "file": p.display().to_string()})
                            .to_string(),
                    ),
                    Err(e) => (500, JSON, serde_json::json!({"error": format!("存档写盘失败: {e}")}).to_string()),
                }
            }
            Err(e) => (400, JSON, serde_json::json!({"error": e}).to_string()),
        },
        ("POST", "/api/load") => {
            let p = save_file();
            match std::fs::read_to_string(&p) {
                Err(e) => (
                    400,
                    JSON,
                    serde_json::json!({"error": format!("没找到存档（{}）: {e}", p.display())}).to_string(),
                ),
                Ok(t) => match serde_json::from_str::<serde_json::Value>(&t) {
                    Err(e) => (400, JSON, serde_json::json!({"error": format!("存档不是 JSON: {e}")}).to_string()),
                    Ok(save) => match Session::load(pack_text, &save) {
                        Ok(ns2) => {
                            *ss = ns2;
                            (200, JSON, ss.state_json().to_string())
                        }
                        Err(e) => (400, JSON, serde_json::json!({"error": e}).to_string()),
                    },
                },
            }
        }
        _ => (404, JSON, "{\"error\":\"无此路\"}".to_string()),
    };
    let _ = respond(stream, out.0, out.1, &out.2);
}

/// 存档落点：$KUYUAN_SAVE_DIR 或 $HOME/.kuyuan（Windows 退 %USERPROFILE%）·单槽。
fn save_file() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("KUYUAN_SAVE_DIR") {
        return std::path::PathBuf::from(d).join("save-slot1.json");
    }
    match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(h) => std::path::PathBuf::from(h).join(".kuyuan").join("save-slot1.json"),
        Err(_) => std::env::temp_dir().join("kuyuan").join("save-slot1.json"),
    }
}

fn query_value(q: &str, key: &str) -> Option<u64> {
    for part in q.split('&') {
        if let Some(v) = part.strip_prefix(&format!("{key}=")) {
            return v.parse().ok();
        }
    }
    None
}

fn parse_act(body: &str) -> Result<(i64, Option<i64>, Option<f64>), String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("请求体不是 JSON: {e}"))?;
    let act = v.get("act").and_then(|x| x.as_i64()).ok_or("缺 act")?;
    let dir = v.get("dir").and_then(|x| x.as_i64());
    let range = v.get("range").and_then(|x| x.as_f64());
    Ok((act, dir, range))
}

fn respond(stream: &mut TcpStream, code: u16, ctype: &str, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reason(code),
        body.as_bytes().len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, String, String, String)> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 2048];
    let hend = loop {
        if let Some(p) = find(&buf, b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 64 * 1024 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "头太大"));
        }
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "连接早断"));
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..hend]).to_string();
    let mut lines = head.split("\r\n");
    let reqline = lines.next().unwrap_or("");
    let mut sp = reqline.split_whitespace();
    let method = sp.next().unwrap_or("").to_string();
    let raw = sp.next().unwrap_or("/").to_string();
    let (path, query) = match raw.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (raw, String::new()),
    };
    let mut clen = 0usize;
    for l in lines {
        let ll = l.to_ascii_lowercase();
        if let Some(rest) = ll.strip_prefix("content-length:") {
            clen = rest.trim().parse().unwrap_or(0);
        }
    }
    let mut body: Vec<u8> = buf[hend + 4..].to_vec();
    while body.len() < clen {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(clen);
    Ok((method, path, query, String::from_utf8_lossy(&body).to_string()))
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn seed_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
        | 1
}