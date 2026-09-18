// 埋点前端 API（事件入本地队列，端点就绪后批量上报）
import { invoke } from "@tauri-apps/api/core";

export function track(
  event: string,
  props: Record<string, unknown> = {},
): Promise<void> {
  return invoke("analytics_track", { event, props });
}

export function installId(): Promise<string> {
  return invoke("analytics_install_id");
}

export const analyticsApi = { track, installId };
