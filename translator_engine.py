"""
Translator Engine - handles translation from multiple sources.
Currently supports Google Translate (no API key required).
"""
import urllib.request
import urllib.parse
import urllib.error
import json
import socket
import subprocess

from network import request_with_strategies, network_hint

try:
    from logger import log
except Exception:
    import logging
    log = logging.getLogger("SbtDeskTran")


LANGUAGES = {
    "Auto Detect": "auto",
    "Afrikaans": "af", "Albanian": "sq", "Amharic": "am",
    "Arabic": "ar", "Armenian": "hy", "Azerbaijani": "az",
    "Basque": "eu", "Belarusian": "be", "Bengali": "bn",
    "Bosnian": "bs", "Bulgarian": "bg", "Catalan": "ca",
    "Cebuano": "ceb", "Chinese (Simplified)": "zh-CN",
    "Chinese (Traditional)": "zh-TW", "Corsican": "co",
    "Croatian": "hr", "Czech": "cs", "Danish": "da",
    "Dutch": "nl", "English": "en", "Esperanto": "eo",
    "Estonian": "et", "Finnish": "fi", "French": "fr",
    "Frisian": "fy", "Galician": "gl", "Georgian": "ka",
    "German": "de", "Greek": "el", "Gujarati": "gu",
    "Haitian Creole": "ht", "Hausa": "ha", "Hawaiian": "haw",
    "Hebrew": "he", "Hindi": "hi", "Hmong": "hmn",
    "Hungarian": "hu", "Icelandic": "is", "Igbo": "ig",
    "Indonesian": "id", "Irish": "ga", "Italian": "it",
    "Japanese": "ja", "Javanese": "jv", "Kannada": "kn",
    "Kazakh": "kk", "Khmer": "km", "Kinyarwanda": "rw",
    "Korean": "ko", "Kurdish": "ku", "Kyrgyz": "ky",
    "Lao": "lo", "Latin": "la", "Latvian": "lv",
    "Lithuanian": "lt", "Luxembourgish": "lb", "Macedonian": "mk",
    "Malagasy": "mg", "Malay": "ms", "Malayalam": "ml",
    "Maltese": "mt", "Maori": "mi", "Marathi": "mr",
    "Mongolian": "mn", "Myanmar": "my", "Nepali": "ne",
    "Norwegian": "no", "Nyanja": "ny", "Odia": "or",
    "Pashto": "ps", "Persian": "fa", "Polish": "pl",
    "Portuguese": "pt", "Punjabi": "pa", "Romanian": "ro",
    "Russian": "ru", "Samoan": "sm", "Scots Gaelic": "gd",
    "Serbian": "sr", "Sesotho": "st", "Shona": "sn",
    "Sindhi": "sd", "Sinhala": "si", "Slovak": "sk",
    "Slovenian": "sl", "Somali": "so", "Spanish": "es",
    "Sundanese": "su", "Swahili": "sw", "Swedish": "sv",
    "Tagalog": "tl", "Tajik": "tg", "Tamil": "ta",
    "Tatar": "tt", "Telugu": "te", "Thai": "th",
    "Turkish": "tr", "Turkmen": "tk", "Ukrainian": "uk",
    "Urdu": "ur", "Uyghur": "ug", "Uzbek": "uz",
    "Vietnamese": "vi", "Welsh": "cy", "Xhosa": "xh",
    "Yiddish": "yi", "Yoruba": "yo", "Zulu": "zu",
}

LANG_CODE_TO_NAME = {v: k for k, v in LANGUAGES.items()}


class TranslationError(Exception):
    pass


def _parse_gtx_response(data) -> tuple:
    """Parse gtx API response. Returns (translated_text, detected_lang)."""
    translated_parts = []
    if data and data[0]:
        for part in data[0]:
            if part and part[0]:
                translated_parts.append(part[0])
    translated = "".join(translated_parts)
    detected = ""
    try:
        if data and len(data) > 2 and data[2]:
            detected = data[2]
    except Exception:
        pass
    return translated, detected


class GoogleTranslateEngine:
    """
    Google Translate engine — no API key required.
    Tries multiple network strategies to work through corporate proxies/firewalls.
    Caches the working strategy index so subsequent calls skip retries.
    """
    name = "Google Translate"

    _ENDPOINTS = [
        "https://translate.googleapis.com/translate_a/single",
        "https://translate.google.com/translate_a/single",
    ]
    _MAX_CHUNK_CHARS = 700

    def __init__(self, working_strategy: int = -1):
        self._working_strategy = working_strategy

    def _split_segment(self, text: str, max_chars: int) -> list:
        chunks = []
        separators = ("\n\n", "\n", ". ", "。", "! ", "? ", "; ", ", ", " ")
        while len(text) > max_chars:
            best = -1
            for sep in separators:
                pos = text.rfind(sep, 0, max_chars)
                if pos > best:
                    best = pos + len(sep)
            if best < max_chars // 3:
                best = max_chars
            chunks.append(text[:best])
            text = text[best:]
        if text:
            chunks.append(text)
        return chunks

    def _split_text(self, text: str, max_chars: int = None) -> list:
        max_chars = max_chars or self._MAX_CHUNK_CHARS
        chunks = []
        current = ""
        for line in text.splitlines(keepends=True) or [text]:
            parts = self._split_segment(line, max_chars)
            for part in parts:
                if current and len(current) + len(part) > max_chars:
                    chunks.append(current)
                    current = ""
                current += part
                if len(current) >= max_chars:
                    chunks.append(current)
                    current = ""
        if current:
            chunks.append(current)
        return chunks

    def _request_endpoint(self, endpoint: str, query: str, strategy: int,
                          settings: dict = None) -> tuple[bytes, int]:
        url = f"{endpoint}?{query}"
        return request_with_strategies(
            url, working_strategy=strategy, settings=settings,
        )

    def _translate_single(self, text: str, src: str = "auto", dest: str = "en",
                          settings: dict = None) -> dict:
        log.debug(f"Translate request: src={src} dest={dest} chars={len(text)}")

        params = {
            "client": "gtx", "sl": src, "tl": dest,
            "dt": ["t", "bd", "ld"], "q": text,
        }
        query = urllib.parse.urlencode(params, doseq=True)

        # Try cached strategy on primary endpoint first
        if self._working_strategy >= 0:
            try:
                raw, _ = self._request_endpoint(
                    self._ENDPOINTS[0], query, self._working_strategy,
                    settings=settings)
                data = json.loads(raw.decode("utf-8"))
                translated, detected = _parse_gtx_response(data)
                return {
                    "translated": translated,
                    "detected_lang": detected or src,
                    "source": self.name,
                    "strategy": self._working_strategy,
                }
            except (urllib.error.URLError, socket.timeout, TimeoutError,
                    OSError, subprocess.SubprocessError) as e:
                log.warning(f"Cached strategy {self._working_strategy} failed: {e}")
                self._working_strategy = -1

        # Full retry across all endpoints
        last_error = None
        for endpoint in self._ENDPOINTS:
            try:
                raw, idx = self._request_endpoint(endpoint, query, -1,
                                                  settings=settings)
                data = json.loads(raw.decode("utf-8"))
                translated, detected = _parse_gtx_response(data)
                if not detected:
                    detected = src
                self._working_strategy = idx
                log.info(f"Translate OK via endpoint {endpoint}, strategy {idx}")
                return {
                    "translated": translated,
                    "detected_lang": detected,
                    "source": self.name,
                    "strategy": idx,
                }
            except (urllib.error.URLError, socket.timeout, TimeoutError,
                    OSError, subprocess.SubprocessError) as e:
                log.warning(f"Endpoint {endpoint} failed: {e}")
                last_error = e
            except Exception as e:
                log.error(f"Endpoint {endpoint} unexpected error: {e}", exc_info=True)
                last_error = e

        raise TranslationError(f"Translation failed: {network_hint(last_error)}")

    def translate(self, text: str, src: str = "auto", dest: str = "en",
                  settings: dict = None) -> dict:
        if not text.strip():
            return {"translated": "", "detected_lang": src, "source": self.name}

        chunks = self._split_text(text)
        if len(chunks) == 1:
            return self._translate_single(text, src=src, dest=dest, settings=settings)

        log.info(f"Large translate request split into {len(chunks)} chunks ({len(text)} chars)")
        translated_parts = []
        detected_lang = src
        last_strategy = -1
        for idx, chunk in enumerate(chunks, start=1):
            log.debug(f"Translating chunk {idx}/{len(chunks)} chars={len(chunk)}")
            result = self._translate_single(chunk, src=src, dest=dest, settings=settings)
            translated_parts.append(result.get("translated", ""))
            detected = result.get("detected_lang")
            if detected and detected != "auto":
                detected_lang = detected
            strategy = result.get("strategy", -1)
            if strategy >= 0:
                last_strategy = strategy
        return {
            "translated": "".join(translated_parts),
            "detected_lang": detected_lang,
            "source": self.name,
            "chunks": len(chunks),
            "strategy": last_strategy,
        }


# Registry of available engines
ENGINES = {
    "Google Translate": GoogleTranslateEngine(),
}


def get_engine(name: str, strategy: int = -1):
    engine = ENGINES.get(name, ENGINES["Google Translate"])
    if strategy >= 0:
        engine._working_strategy = strategy
    return engine
