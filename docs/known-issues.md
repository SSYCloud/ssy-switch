# 已知问题（非阻塞，跟踪用）

## mcp_commands 测试套件在 macOS 下批量失败（2026-09-19 记录）

- 现象：`cargo test --test mcp_commands` 21/23 失败，根因 `mcp_commands.rs:65`
  "importing default config should persist to cc-switch.db"——测试对 home 落盘
  路径的假设与 macOS 实际路径不一致，首例失败毒化共享互斥锁后级联放大
  （并行重跑失败数在 16~21 间浮动）。
- 鉴定：先在问题（ssy-sdk 分支基线 `7a3dfd83` 复跑同样失败），与 SSY 规则层/
  充值/统计改动无关（该套件不触碰 ssy-core）。
- 影响：仅测试基建；功能不受影响。
- 修复方向：测试 setup 改用与生产一致的 home 解析，或用 tempfile 隔离；
  修复时移除本条。
