#!/usr/bin/env bash
# 胜算云充值支付 E2E 测试脚本
#
# 用法：
#   ./scripts/pay-e2e-test.sh [金额，单位元，默认 10]
#
# 流程：创建充值订单 → 终端渲染收款二维码 → 轮询支付状态 →
#       支付成功后打印账单与余额验证。
#
# 依赖：sqlite3、curl、python3；qrencode 可选（终端渲染二维码，未装则自动打开浏览器）。
# 注意：会创建真实可支付的订单，扫码付款前请确认金额。

set -euo pipefail

AMOUNT_YUAN="${1:-10}"
AMOUNT_UNITS=$((AMOUNT_YUAN * 1000))   # 实测口径：10000 → ¥10
DB="$HOME/.ssy-switch/ssy-switch.db"
API="https://api.shengsuanyun.com"
POLL_INTERVAL=3
POLL_MAX=100   # 3s × 100 ≈ 5 分钟

command -v sqlite3 >/dev/null || { echo "需要 sqlite3"; exit 1; }
command -v python3 >/dev/null || { echo "需要 python3"; exit 1; }

JWT=$(sqlite3 "$DB" "SELECT jwt_token FROM shengsuanyun_credentials LIMIT 1;")
[ -n "$JWT" ] || { echo "未找到登录凭据，请先在 SSY-Switch 中登录胜算云"; exit 1; }

ORDER_ID=$(openssl rand -hex 8 2>/dev/null || python3 -c "import secrets;print(secrets.token_hex(8))")

echo "── 1) 创建充值订单：¥${AMOUNT_YUAN}（alipay, amounts=${AMOUNT_UNITS}）"
RESP=$(curl -s --max-time 20 -X POST "$API/user/recharge" \
  -H "x-token: $JWT" -H "Content-Type: application/json" \
  -d "{\"amounts\":$AMOUNT_UNITS,\"payWay\":\"alipay\",\"recahrgeId\":22,\"orderId\":\"$ORDER_ID\"}")

read -r ORDER_URL SRV_ORDER_ID PRICE <<< "$(echo "$RESP" | python3 -c "
import json,sys
d=json.load(sys.stdin)
assert d.get('code')==0, f\"创建失败: {d.get('msg')}\"
po=d['data']['payOrder']
print(d['data']['url'], po['OrderID'], po['Price'])
")"
echo "   服务端订单: $SRV_ORDER_ID（Price=$PRICE → ¥$(python3 -c "print(f'{$PRICE/10000:.2f}')")）"
echo "   收款链接: $ORDER_URL"

echo "── 2) 渲染收款二维码"
if command -v qrencode >/dev/null; then
  qrencode -t ANSIUTF8 "$ORDER_URL"
else
  echo "   （未安装 qrencode，改用浏览器打开付款页）"
  open "$ORDER_URL" 2>/dev/null || echo "   请手动打开上面的收款链接"
fi

echo "── 3) 轮询支付状态（每 ${POLL_INTERVAL}s，最多 $((POLL_MAX * POLL_INTERVAL / 60)) 分钟）"
echo "   用支付宝/微信扫上方二维码完成付款 ¥${AMOUNT_YUAN} …"

for i in $(seq 1 "$POLL_MAX"); do
  RESP=$(curl -s --max-time 10 -X POST "$API/user/payQuery" \
    -H "x-token: $JWT" -H "Content-Type: application/json" \
    -d "{\"orderId\":\"$SRV_ORDER_ID\"}")
  MSG=$(echo "$RESP" | python3 -c "
import json,sys
d=json.load(sys.stdin)
print(d.get('msg') or ('ok' if d.get('code')==0 else 'error'))
")
  if [ "$MSG" != "unpaid" ]; then
    echo ""
    echo "── 4) 支付状态变化：$MSG（第 $i 次轮询）"
    break
  fi
  printf "\r   [%3d] %s" "$i" "$MSG"
  sleep "$POLL_INTERVAL"
done

if [ "${MSG:-unpaid}" = "unpaid" ]; then
  echo ""
  echo "✗ 轮询超时仍未支付（订单未扣款，可忽略或到控制台查看）"
  exit 1
fi

echo "── 5) 到账验证"
echo "最近一笔账单："
curl -s --max-time 10 -X POST "$API/userorder/billlist" \
  -H "x-token: $JWT" -H "Content-Type: application/json" \
  -d '{"page":1,"page_size":1}' | python3 -c "
import json,sys
d=json.load(sys.stdin)
b=(d.get('data') or {}).get('bills', [])
if not b: print('  （无账单——支付可能仍在清算，稍后手动核对）')
else:
  b=b[0]
  print(f\"  {b['CreatedAt'][:19]}  Asset={b['Asset']} (1e-4 元 → ¥{b['Asset']/10000:.2f})  类型={b['BillType']}/{b['BillSubType']}\")
  print(f\"  余额快照: ¥{b['Balance']/10000:.2f}\")
"
echo "当前网关余额："
curl -s --max-time 10 "https://router.shengsuanyun.com/api/v1/balance" \
  -H "Authorization: Bearer $(sqlite3 "$DB" "SELECT api_key FROM shengsuanyun_credentials LIMIT 1;")" | python3 -c "
import json,sys
d=json.load(sys.stdin).get('data',{})
print(f\"  account_balance_cny = {d.get('account_balance_cny')}\")
print(f\"  voucher_balance_cny = {d.get('voucher_balance_cny')}\")
"
echo "✓ 测试完成"
