"""
Diff Engine - computes word-level and line-level diffs
similar to GitHub / VSCode / Beyond Compare style.
Supports CJK (Japanese, Chinese, Korean) character-level tokenization.
"""
import difflib
import re
from dataclasses import dataclass, field
from typing import List, Tuple


@dataclass
class DiffChunk:
    """Represents a chunk of diff."""
    kind: str          # 'equal' | 'insert' | 'delete' | 'replace'
    old_lines: List[str] = field(default_factory=list)
    new_lines: List[str] = field(default_factory=list)
    old_start: int = 0
    new_start: int = 0


@dataclass
class InlineToken:
    """Word-level token within a changed line."""
    text: str
    kind: str   # 'equal' | 'insert' | 'delete'


# CJK codepoint ranges — check bằng ord() nhanh hơn regex
_CJK_RANGES = (
    (0x3000, 0x303f),   # CJK symbols & punctuation
    (0x3040, 0x309f),   # Hiragana
    (0x30a0, 0x30ff),   # Katakana
    (0x31f0, 0x31ff),   # Katakana phonetic extensions
    (0xff00, 0xffef),   # Halfwidth/fullwidth forms
    (0x4e00, 0x9fff),   # CJK Unified Ideographs
    (0x3400, 0x4dbf),   # CJK Extension A
    (0x20000, 0x2a6df), # CJK Extension B
    (0xac00, 0xd7af),   # Hangul syllables
    (0x1100, 0x11ff),   # Hangul Jamo
    (0xa960, 0xa97f),   # Hangul Jamo Extended-A
    (0xd7b0, 0xd7ff),   # Hangul Jamo Extended-B
)


def _is_cjk_char(ch: str) -> bool:
    """Return True if character belongs to a CJK/Hangul script."""
    cp = ord(ch)
    return any(lo <= cp <= hi for lo, hi in _CJK_RANGES)


def _is_word_char(ch: str) -> bool:
    return ch == "_" or ch.isalnum()


def tokenize_inline(line: str) -> List[str]:
    """
    Smart tokenizer:
    - CJK scripts (Japanese, Chinese, Korean): each character is a separate token
    - Latin / other scripts: word chars kept together, punctuation split
    - Mixed lines handled correctly
    """
    tokens: List[str] = []
    buf = ""        # accumulates word chars

    for ch in line:
        if _is_cjk_char(ch):
            if buf:
                tokens.append(buf)
                buf = ""
            tokens.append(ch)   # each CJK char is its own token
        elif _is_word_char(ch):
            buf += ch
        else:
            if buf:
                tokens.append(buf)
                buf = ""
            tokens.append(ch)

    if buf:
        tokens.append(buf)

    return tokens


def _normalize_whitespace_for_diff(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def _can_char_diff_token(text: str) -> bool:
    return bool(text) and not text.isspace() and not any(_is_cjk_char(ch) for ch in text)


def _append_char_diff(
    old_text: str,
    new_text: str,
    old_result: List[InlineToken],
    new_result: List[InlineToken],
):
    matcher = difflib.SequenceMatcher(None, old_text, new_text, autojunk=False)
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            if i2 > i1:
                old_result.append(InlineToken(old_text[i1:i2], "equal"))
            if j2 > j1:
                new_result.append(InlineToken(new_text[j1:j2], "equal"))
        elif tag == "insert":
            if j2 > j1:
                new_result.append(InlineToken(new_text[j1:j2], "insert"))
        elif tag == "delete":
            if i2 > i1:
                old_result.append(InlineToken(old_text[i1:i2], "delete"))
        elif tag == "replace":
            if i2 > i1:
                old_result.append(InlineToken(old_text[i1:i2], "delete"))
            if j2 > j1:
                new_result.append(InlineToken(new_text[j1:j2], "insert"))


def compute_line_diff(old_text: str, new_text: str, ignore_whitespace: bool = False) -> List[DiffChunk]:
    """Compute line-by-line diff between two texts."""
    old_lines = old_text.splitlines(keepends=True)
    new_lines = new_text.splitlines(keepends=True)

    if old_lines and not old_lines[-1].endswith("\n"):
        old_lines[-1] += "\n"
    if new_lines and not new_lines[-1].endswith("\n"):
        new_lines[-1] += "\n"

    old_compare = old_lines
    new_compare = new_lines
    if ignore_whitespace:
        old_compare = [_normalize_whitespace_for_diff(line) for line in old_lines]
        new_compare = [_normalize_whitespace_for_diff(line) for line in new_lines]

    matcher = difflib.SequenceMatcher(None, old_compare, new_compare, autojunk=False)
    chunks: List[DiffChunk] = []

    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        chunks.append(DiffChunk(
            kind=tag,
            old_lines=old_lines[i1:i2],
            new_lines=new_lines[j1:j2],
            old_start=i1,
            new_start=j1,
        ))

    return chunks


def compute_inline_diff(
    old_line: str, new_line: str, word_diff: bool = True
) -> Tuple[List[InlineToken], List[InlineToken]]:
    """Compute inline diff for a pair of changed lines.

    When word_diff is True, changed Latin tokens are highlighted as whole words.
    When False, changed Latin tokens can be refined to character-level spans.
    """
    old_tokens = tokenize_inline(old_line.rstrip("\n"))
    new_tokens = tokenize_inline(new_line.rstrip("\n"))

    matcher = difflib.SequenceMatcher(None, old_tokens, new_tokens, autojunk=False)

    old_result: List[InlineToken] = []
    new_result: List[InlineToken] = []

    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            for t in old_tokens[i1:i2]:
                old_result.append(InlineToken(t, "equal"))
            for t in new_tokens[j1:j2]:
                new_result.append(InlineToken(t, "equal"))
        elif tag == "insert":
            for t in new_tokens[j1:j2]:
                new_result.append(InlineToken(t, "insert"))
        elif tag == "delete":
            for t in old_tokens[i1:i2]:
                old_result.append(InlineToken(t, "delete"))
        elif tag == "replace":
            old_slice = old_tokens[i1:i2]
            new_slice = new_tokens[j1:j2]
            if (
                not word_diff
                and len(old_slice) == 1 and len(new_slice) == 1
                and _can_char_diff_token(old_slice[0])
                and _can_char_diff_token(new_slice[0])
            ):
                _append_char_diff(old_slice[0], new_slice[0], old_result, new_result)
            else:
                for t in old_slice:
                    old_result.append(InlineToken(t, "delete"))
                for t in new_slice:
                    new_result.append(InlineToken(t, "insert"))

    return old_result, new_result


def get_diff_stats(chunks: List[DiffChunk]) -> dict:
    """Return summary statistics for a diff."""
    added   = sum(len(c.new_lines) for c in chunks if c.kind in ("insert", "replace"))
    removed = sum(len(c.old_lines) for c in chunks if c.kind in ("delete", "replace"))
    changed = sum(1 for c in chunks if c.kind != "equal")
    return {"added": added, "removed": removed, "changed_blocks": changed}
