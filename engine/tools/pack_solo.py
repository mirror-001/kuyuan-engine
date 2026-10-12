#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""单机版打包器：page.html + k32-local.js + release wasm → 单文件 HTML。

用法：
    python3 tools/pack_solo.py [--wasm 路径] [--out 路径]

默认：
    wasm = /root/build/engine/wasm32-unknown-unknown/release/kuyuan_wasm.wasm
    out  = <工作区父目录>/32×32/分发/32x32-单机版.html

前置（改过页面/桥之后）：
    cd engine && CARGO_TARGET_DIR=/root/build/engine \\
        cargo build --release --target wasm32-unknown-unknown -p kuyuan-wasm
"""
import argparse
import base64
import io
import os

HERE = os.path.dirname(os.path.abspath(__file__))   # engine/tools
ENGINE = os.path.dirname(HERE)                      # engine
GAME = os.path.dirname(os.path.dirname(ENGINE))     # 工作区父目录
SRC = os.path.join(ENGINE, 'play', 'src')
DEF_WASM = '/root/build/engine/wasm32-unknown-unknown/release/kuyuan_wasm.wasm'
DEF_OUT = os.path.join(GAME, '32×32', '分发', '32x32-单机版.html')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--wasm', default=DEF_WASM)
    ap.add_argument('--out', default=DEF_OUT)
    a = ap.parse_args()

    page = io.open(os.path.join(SRC, 'page.html'), encoding='utf-8', newline='').read()
    bridge = io.open(os.path.join(SRC, 'k32-local.js'), encoding='utf-8', newline='').read()
    wasm_b64 = base64.b64encode(open(a.wasm, 'rb').read()).decode('ascii')

    inject = ('<script>window.K32_WASM_BASE64="' + wasm_b64 + '";</script>\n'
              '<script>\n' + bridge + '\n</script>\n')
    assert page.count('</body>') == 1, '页面里 </body> 不是恰好一处？'

    out = page.replace('</body>', inject + '</body>')
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    io.open(a.out, 'w', encoding='utf-8', newline='').write(out)
    print('打包完成：', a.out)
    print('总字符：', len(out), '· wasm base64：', len(wasm_b64))


if __name__ == '__main__':
    main()