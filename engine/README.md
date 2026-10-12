# 库元引擎 · KUYUAN ENGINE

一个把**世界放进数据里**的通用引擎：**内容包（JSON）声明世界**，引擎负责 **构造 · 推进 · 观测**。
不含任何游戏名词——逻辑只住一处（Rust 核心），内容全部是数据。

## 特性
- **内容即数据**：世界＝若干内容包；换内容不改引擎。
- **确定性**：固定步长；同输入必同输出。
- **事件流＝唯一事实源**：只追加；「初值 ＋ 事件流 ＝ 终态」可重演。
- **最小 AST**：公式＝JSON 数组（算术／比较／逻辑／分支），不引入脚本语言。
- **加载期门禁**：能拦的错误不留给运行期（地址 · 引用 · 单写者 · 类型）。
- **观测带视角**：三档可见性（真／私／裁判）；投影只发生在观测侧。
- **存档／回放／回滚**：引用式＋重算式（指纹 · 封条）；重演与原始运行逐字节一致。
- **有界运行**：注入两道（类型 · 界式）＋ 时间拍上限；拒绝且记入日志。

## 结构（仓库根）
- `engine/` —— Rust 工作区：
  `crates/`（core · registry · graph · build · eval · log · driver · view · store · guard）· `cli/` · `play/` · `arena/` · `crates/verify/`（测试网）· `crates/wasm/`（浏览器胶水）
- `packs/` —— 示例内容包（demo · 32×32 对局）
- `LICENSE` —— MIT

## 构建与自测
需要 Rust（edition 2024；rustc ≥ 1.85）。
```sh
cd engine
cargo build
cargo test -p kuyuan-verify   # 回归（110 条）
cargo test                    # 全库（含 play／arena）
```

## 跑一局
```sh
cd engine
cargo run -p kuyuan-cli   -- run "../packs/demo.计数器.json" --ticks 2
cargo run -p kuyuan-cli   -- run "../packs/32×32/duel32.json" --ticks 42
cargo run -p kuyuan-play  -- "../packs/32×32/duel32.json"      # 打开 http://127.0.0.1:8092/
cargo run -p kuyuan-arena -- --pack "../packs/32×32/duel32.json" --matches 50 --p1 r --p2 r
```

## 许可
MIT，见 `LICENSE`。
