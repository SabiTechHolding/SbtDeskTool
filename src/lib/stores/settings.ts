import { invoke } from "@tauri-apps/api/core";

export interface AppSettings {
  theme: "dark" | "light";
  layout: "horizontal" | "vertical";
  always_on_top: boolean;
  compact_mode: boolean;
  engine: string;
  src_lang: string;
  dest_lang: string;
  tran_source_text: string;
  font_size_tran: number;
  font_size_diff: number;
  font_size_note: number;
  active_tab: string;
  diff_word_diff: boolean;
  diff_ignore_whitespace: boolean;
  diff_show_whitespace: boolean;
  tran_show_whitespace: boolean;
  note_show_whitespace: boolean;
  diff_algorithm: "legacy" | "advanced";
  note_auto_save: boolean;
  word_wrap: boolean;
  word_wrap_tran: boolean;
  word_wrap_diff: boolean;
  word_wrap_note: boolean;
  window_effect: string;
  window_width: number;
  window_height: number;
  network_strategy: number;
  sash_pos_tran: number;
  compact_width: number;
  compact_height: number;
  compact_diff_height: number;
  tran_find_case: boolean;
  tran_find_word: boolean;
  tran_find_regex: boolean;
  diff_find_case: boolean;
  diff_find_word: boolean;
  diff_find_regex: boolean;
  diff_left_ratio: number;
  note_sidebar_width: number;
}

const DEFAULT_SETTINGS: AppSettings = {
  theme: "dark",
  layout: "horizontal",
  always_on_top: false,
  compact_mode: false,
  engine: "Google Translate",
  src_lang: "Auto Detect",
  dest_lang: "English",
  tran_source_text: "",
  font_size_tran: 10,
  font_size_diff: 10,
  font_size_note: 10,
  active_tab: "diff",
  diff_word_diff: true,
  diff_ignore_whitespace: false,
  diff_show_whitespace: false,
  tran_show_whitespace: false,
  note_show_whitespace: false,
  diff_algorithm: "legacy",
  note_auto_save: true,
  word_wrap: true,
  word_wrap_tran: true,
  word_wrap_diff: true,
  word_wrap_note: true,
  window_effect: "blur",
  window_width: 980,
  window_height: 640,
  network_strategy: 0,
  sash_pos_tran: 50,
  compact_width: 500,
  compact_height: 240,
  compact_diff_height: 280,
  tran_find_case: false,
  tran_find_word: false,
  tran_find_regex: false,
  diff_find_case: false,
  diff_find_word: false,
  diff_find_regex: false,
  diff_left_ratio: 0.5,
  note_sidebar_width: 220,
};

let cached: AppSettings | null = null;

export async function loadSettings(): Promise<AppSettings> {
  if (cached) return cached;
  try {
    cached = await invoke<AppSettings>("get_settings");
    return merged();
  } catch {
    return merged();
  }
}

export async function saveSetting<K extends keyof AppSettings>(
  key: K,
  value: AppSettings[K],
) {
  if (cached) cached[key] = value;
  try {
    await invoke("save_setting", { key, value });
  } catch {
    // fall back to in-memory
  }
}

function merged(): AppSettings {
  return { ...DEFAULT_SETTINGS, ...cached };
}
