export function detectEditorLanguage(text: string, fallback = "markdown") {
  const sample = text.trimStart();
  if (!sample) return fallback;

  if (sample.startsWith("{") || sample.startsWith("[")) {
    try {
      JSON.parse(text);
      return "json";
    } catch {
      // Keep detecting non-JSON content below.
    }
  }

  if (/^<(!doctype|html|[A-Za-z][\w:-]*[\s>])/i.test(sample)) return "html";
  if (/^(#\s|##\s|`{3}|---\s*$)/m.test(sample)) return "markdown";
  if (/^(use|mod|pub|fn|impl|struct|enum|trait)\b/m.test(sample) || /\blet\s+mut\b/.test(sample)) {
    return "rust";
  }
  if (/[.#][\w-]+\s*\{[^}]*:[^}]*\}/s.test(sample)) return "css";
  if (
    /\b(interface|export|const|let|function)\b/.test(sample) ||
    /\btype\s+[A-Za-z_$][\w$]*\s*=/.test(sample) ||
    /\bclass\s+[A-Za-z_$][\w$]*\s*\{/.test(sample) ||
    /^\s*import\s+(?:type\s+)?(?:\{[^}]*\}|\*\s+as\s+[A-Za-z_$][\w$]*|[A-Za-z_$][\w$]*(?:\s*,\s*\{[^}]*\})?)\s+from\s+["']/m.test(sample)
  ) {
    return "typescript";
  }
  if (/^(from\s+\S+\s+import|import\s+[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*|def\s+\w+\s*\(|class\s+\w+(?:\([^)]*\))?\s*:)/m.test(sample)) {
    return "python";
  }

  return fallback;
}
