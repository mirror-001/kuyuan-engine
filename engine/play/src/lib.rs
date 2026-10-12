//! 库元引擎 · play（库入口）——会话核，供 wasm / 其他壳复用。
//!
//! `kuyuan-play` 的二进制（main.rs）走极简 HTTP；本库出口只有会话核：
//! `session::{Session, Opponent}`——无任何 I/O 依赖（可编译到 wasm）。

pub mod session;