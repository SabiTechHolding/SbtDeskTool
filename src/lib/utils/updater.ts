import { invoke } from "@tauri-apps/api/core";

export interface DialogRequest {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  showCancel?: boolean;
  tone?: "normal" | "warning" | "error";
}

export type ShowDialog = (request: DialogRequest) => Promise<boolean>;

const UPDATE_ENDPOINT =
  "https://github.com/SabiTechHolding/SbtDeskTool/releases/latest/download/latest.json";

export async function checkForUpdates(
  force: boolean,
  onProgress?: (message: string) => void,
  showDialog?: ShowDialog,
) {
  try {
    onProgress?.("Checking...");
    const { check } = await import("@tauri-apps/plugin-updater");
    const proxy = await invoke<string | null>("resolve_system_proxy", {
      url: UPDATE_ENDPOINT,
    }).catch(() => null);
    const update = await check({
      timeout: 15000,
      ...(proxy ? { proxy } : {}),
    });
    if (!update) {
      if (force) await showDialog?.({ title: "Check Update", message: "You are up to date." });
      return;
    }
    if (!showDialog) return;
    const accepted = await showDialog({
      title: `Update ${update.version}`,
      message: `A new version is available. Download and install now?${update.body ? `\n\n${update.body}` : ""}`,
      confirmLabel: "Install",
      cancelLabel: "Later",
      showCancel: true,
    });
    if (!accepted) {
      return;
    }
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") onProgress?.("Downloading...");
      else if (event.event === "Progress") onProgress?.("Downloading update...");
      else onProgress?.("Installing...");
    });
    await invoke("restart_app");
  } catch {
    if (force) {
      await showDialog?.({
        title: "Check Update",
        message: "Unable to check for updates. Please try again later.",
        tone: "error",
      });
    }
  } finally {
    onProgress?.("");
  }
}
