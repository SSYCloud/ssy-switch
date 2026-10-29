import { getVersion } from "@tauri-apps/api/app";

export interface UpdateInfo {
  currentVersion: string;
  availableVersion: string;
  notes?: string;
  pubDate?: string;
}

export interface CheckOptions {
  timeout?: number;
}

export async function getCurrentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "";
  }
}

export async function checkForUpdate(
  opts: CheckOptions = {},
): Promise<
  { status: "up-to-date" } | { status: "available"; info: UpdateInfo }
> {
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check({ timeout: opts.timeout ?? 30000 });

  if (!update) {
    return { status: "up-to-date" };
  }

  try {
    return {
      status: "available",
      info: {
        currentVersion: update.currentVersion,
        availableVersion: update.version,
        notes: update.body,
        pubDate: update.date,
      },
    };
  } finally {
    // Installation runs in Rust so replacing the app never depends on the old WebView.
    await update.close();
  }
}
