#!/usr/bin/env python3
"""胜算云充值支付测试网页服务（本地工具）。

用法：
    python3 scripts/pay-web-test.py [端口]     # 默认 18787

然后浏览器打开 http://127.0.0.1:<端口>：
    输入金额 → 创建订单 → 页面显示收款二维码 → 扫码支付 →
    页面自动轮询支付状态，到账后显示账单与余额。

安全说明：
    - 只监听 127.0.0.1，不对局域网开放
    - JWT / API Key 只在本服务进程内使用，不下发到页面
    - 二维码通过 jsdelivr 的 qrcode 库在浏览器本地生成（收款链接不经过第三方）
"""

import json
import re
import secrets
import sqlite3
import sys
import io
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import qrcode
import qrcode.image.svg

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 18787
DB = Path.home() / ".ssy-switch" / "ssy-switch.db"
API = "https://api.shengsuanyun.com"

PAGE = """<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8">
<title>胜算云充值测试</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
 body{font:15px/1.7 -apple-system,sans-serif;background:#f8fafc;color:#1f2937;
      display:grid;place-items:center;min-height:100vh;margin:0}
 .card{width:min(420px,calc(100vw - 32px));background:#fff;border:1px solid #e5e7eb;
       border-radius:12px;padding:28px 24px;box-shadow:0 8px 24px rgba(15,23,42,.06)}
 h1{font-size:18px;margin:0 0 16px}
 input,button{font:inherit;border-radius:8px}
 input{width:100%;box-sizing:border-box;padding:9px 12px;border:1px solid #d1d5db}
 button{width:100%;padding:10px;margin-top:12px;border:0;background:#059669;color:#fff;
        cursor:pointer;font-weight:600}
 button:disabled{opacity:.5}
 #qr{display:grid;place-items:center;min-height:180px;margin:12px 0}
 #qr img{width:180px;height:180px}
 .ok{color:#059669;font-weight:600}
 .err{color:#dc2626}
 .muted{color:#6b7280;font-size:13px}
 table{width:100%;font-size:13px;border-collapse:collapse;margin-top:8px}
 td,th{border-top:1px solid #eee;padding:4px 2px;text-align:left}
 th{color:#6b7280;font-weight:500}
</style></head><body>
<div class="card">
  <h1>胜算云充值支付测试</h1>
  <label>金额（元，最低 30）<input id="amount" type="number" min="30" value="30"></label>
  <button id="go" onclick="createOrder()">创建订单并显示二维码</button>
  <div id="qr"></div>
  <div id="status" class="muted"></div>
  <div id="result"></div>
</div>
<script>
let timer = null;
const $ = (id) => document.getElementById(id);
const yuan = (raw) => "¥" + (raw / 10000).toFixed(2);

async function api(path, body) {
  const r = await fetch(path, body ? {
    method: "POST", headers: {"Content-Type": "application/json"},
    body: JSON.stringify(body),
  } : undefined);
  return r.json();
}

async function createOrder() {
  const amount = parseInt($("amount").value, 10);
  if (!(amount >= 30)) {
    $("status").innerHTML = '<span class="err">最低充值金额为 ¥30</span>';
    return;
  }
  $("go").disabled = true; $("qr").innerHTML = ""; $("result").innerHTML = "";
  $("status").textContent = "创建订单中…";
  try {
    const o = await api("/api/order", { amount });
    $("status").innerHTML = '订单 <b>' + o.order_id + '</b> 待支付（¥' + amount + '）';
    $("qr").innerHTML =
      '<img alt="收款码" src="/api/qr?data=' + encodeURIComponent(o.url) + '">' +
      '<div class="muted">支付宝扫一扫</div>';
    $("status").innerHTML += ' · <a href="' + o.url + '" target="_blank">在浏览器打开</a>';
    poll(o.order_id, 0);
  } catch (e) {
    $("status").innerHTML = '<span class="err">' + e + '</span>';
    $("go").disabled = false;
  }
}

async function poll(orderId, n) {
  if (n > 100) { $("status").textContent = "轮询超时（订单未扣款，可忽略）"; $("go").disabled = false; return; }
  const s = await api("/api/status?order_id=" + encodeURIComponent(orderId));
  if (s.status === "unpaid") {
    $("status").innerHTML = '订单 <b>' + orderId + '</b> 待支付 · 已轮询 ' + (n + 1) + ' 次';
    timer = setTimeout(() => poll(orderId, n + 1), 3000);
    return;
  }
  $("status").innerHTML = '支付状态：<b>' + s.status + '</b>';
  const bal = await api("/api/balance");
  const bill = (await api("/api/bill?limit=1")).bills || [];
  const rows = bal ? '<tr><th>账户余额</th><td>¥' + bal.account_balance_cny + '</td></tr>' +
                     '<tr><th>体验券</th><td>¥' + bal.voucher_balance_cny + '</td></tr>' +
                     '<tr><th>待扣金额</th><td>¥' + bal.locked_balance_cny + '</td></tr>' : "";
  const billRow = bill[0] ? '<tr><th>最近账单</th><td>' + bill[0].CreatedAt.slice(0,19) +
    ' · ' + bill[0].BillSubType + ' · ' + yuan(bill[0].Asset) + '</td></tr>' : "";
  $("result").innerHTML = '<p class="ok">支付流程完成 ✓</p><table>' + rows + billRow + '</table>';
  $("go").disabled = false;
}
</script></body></html>"""


def db_token(query: str) -> str:
    con = sqlite3.connect(DB)
    try:
        row = con.execute(query).fetchone()
        return (row[0] if row else "") or ""
    finally:
        con.close()


def jwt_token() -> str:
    return db_token("SELECT jwt_token FROM shengsuanyun_credentials LIMIT 1;")


def api_key() -> str:
    return db_token("SELECT api_key FROM shengsuanyun_credentials LIMIT 1;")


def ssy_json(req: urllib.request.Request) -> dict:
    with urllib.request.urlopen(req, timeout=20) as r:
        return json.loads(r.read().decode())


class Handler(BaseHTTPRequestHandler):
    def _json(self, obj: dict, code: int = 200) -> None:
        body = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        full = self.path
        path = full.split("?")[0]
        try:
            if path in ("/", "/index.html"):
                body = PAGE.encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            elif path == "/api/status":
                order_id = re.search(r"order_id=([^&]+)", self.path).group(1)
                d = ssy_json(urllib.request.Request(
                    f"{API}/user/payQuery",
                    data=json.dumps({"orderId": order_id}).encode(),
                    headers={"x-token": jwt_token(),
                             "Content-Type": "application/json"},
                    method="POST",
                ))
                self._json({"status": d.get("msg") or "unknown"})
            elif path == "/api/qr":
                m = re.search(r"data=([^&]+)", full)
                if not m:
                    return self._json({"error": "missing data"}, 400)
                data = urllib.parse.unquote(m.group(1))
                img = qrcode.make(data, image_factory=qrcode.image.svg.SvgPathImage,
                                  box_size=10, border=2)
                buf = io.BytesIO()
                img.save(buf)
                body = buf.getvalue()
                self.send_response(200)
                self.send_header("Content-Type", "image/svg+xml")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            elif path == "/api/balance":
                d = ssy_json(urllib.request.Request(
                    f"{API.replace('api.shengsuanyun', 'router.shengsuanyun')}"
                    "/api/v1/balance",
                    headers={"Authorization": f"Bearer {api_key()}"},
                ))
                self._json(d.get("data", {}))
            else:
                self._json({"error": "not found"}, 404)
        except Exception as e:  # noqa: BLE001
            self._json({"error": str(e)}, 502)

    def do_POST(self) -> None:
        length = int(self.headers.get("Content-Length") or 0)
        payload = json.loads(self.rfile.read(length) or b"{}")

        try:
            if self.path == "/api/order":
                amount = int(payload.get("amount", 0))
                if amount < 30:
                    return self._json({"error": "最低充值金额为 ¥30（后端限制 code 70002）"}, 400)
                data = ssy_json(urllib.request.Request(
                    f"{API}/user/recharge",  # noqa: SSY 自定义金额最低 ¥30（code 70002）
                    data=json.dumps({
                        "amounts": amount * 1000,       # 实测口径：10000 → ¥10
                        "payWay": "alipay",
                        "recahrgeId": 22,
                        "orderId": secrets.token_hex(8),
                    }).encode(),
                    headers={"x-token": jwt_token(),
                             "Content-Type": "application/json"},
                    method="POST",
                ))["data"]
                return self._json({"order_id": data["payOrder"]["OrderID"],
                                   "url": data["url"]})

            if self.path.startswith("/api/bill"):
                d = ssy_json(urllib.request.Request(
                    f"{API}/userorder/billlist",
                    data=json.dumps({"page": 1,
                                     "page_size": int(payload.get("limit", 1)) or 1}
                                    ).encode(),
                    headers={"x-token": jwt_token(),
                             "Content-Type": "application/json"},
                    method="POST",
                ))
                return self._json({"bills": (d.get("data") or {}).get("bills", [])})

            return self._json({"error": "not found"}, 404)
        except Exception as e:  # noqa: BLE001 — 本地测试工具，错误原样回显
            return self._json({"error": str(e)}, 502)

    def log_message(self, fmt: str, *args) -> None:  # 静默访问日志
        pass


if __name__ == "__main__":
    print(f"充值测试页: http://127.0.0.1:{PORT}  （Ctrl+C 退出）")
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
