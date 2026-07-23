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

interface UpdateMetadata {
  rid: number;
  currentVersion: string;
  version: string;
  date?: string;
  body?: string;
  rawJson: Record<string, unknown>;
}

const UPDATE_ENDPOINT =
  "https://github.com/SabiTechHolding/SbtDeskTool/releases/latest/download/latest.json";

function errorMessage(error: unknown) {
  if (error instanceof Error) return `${error.name}: ${error.message}`;
  return String(error);
}

export async function checkForUpdates(
  force: boolean,
  onProgress?: (message: string) => void,
  showDialog?: ShowDialog,
) {
  try {
    onProgress?.("Checking...");
    const { Update } = await import("@tauri-apps/plugin-updater");
    const metadata = await invoke<UpdateMetadata | null>("check_for_update", {
      timeout: 20000,
    });
    const update = metadata ? new Update(metadata) : null;
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
  } catch (error) {
    const detail = `${UPDATE_ENDPOINT} - ${errorMessage(error)}`;
    console.error("Update check failed", error);
    await invoke("record_update_error", { message: detail }).catch(() => undefined);
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
