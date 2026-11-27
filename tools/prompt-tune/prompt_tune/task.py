"""Calls the translation model the way niinii's chat backend does.

Request parameters come from the `[chat]` section of niinii.toml so the
tuner measures the configuration the app actually runs with.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import sqlite3
import threading
from dataclasses import dataclass
from pathlib import Path

import httpx


def find_config(repo_root: Path) -> Path | None:
    """niinii reads ./niinii.toml first, then <config dir>/niinii/niinii.toml."""
    candidates = [repo_root / "niinii.toml"]
    if appdata := os.environ.get("APPDATA"):
        candidates.append(Path(appdata) / "niinii" / "niinii.toml")
    candidates.append(Path.home() / ".config" / "niinii" / "niinii.toml")
    return next((p for p in candidates if p.exists()), None)


@dataclass
class ChatParams:
    endpoint: str
    model: str
    api_key: str
    temperature: float | None
    top_p: float | None
    max_tokens: int | None
    presence_penalty: float | None
    service_tier: str | None
    reasoning_effort: str | None
    verbosity: str | None
    stream: bool
    timeout_s: float

    @classmethod
    def from_config(cls, config: dict, endpoint: str | None, model: str | None) -> "ChatParams":
        chat = config.get("chat", {})
        # niinii.toml's key belongs to its configured endpoint; never send it
        # to an endpoint given on the command line.
        api_key = os.environ.get("NIINII_TUNE_API_KEY") or ("" if endpoint else config.get("openai_api_key", ""))
        return cls(
            endpoint=(endpoint or config.get("openai_api_endpoint", "")).rstrip("/"),
            model=model or config.get("openai_model", ""),
            api_key=api_key,
            temperature=chat.get("temperature"),
            top_p=chat.get("top_p"),
            max_tokens=chat.get("max_tokens", 128),
            presence_penalty=chat.get("presence_penalty"),
            service_tier=chat.get("service_tier", "priority"),
            reasoning_effort=chat.get("reasoning_effort"),
            verbosity=chat.get("verbosity"),
            stream=chat.get("stream", True),
            timeout_s=max(chat.get("timeout", 10000) / 1000, 60.0),
        )

    def body(self, messages: list[dict]) -> dict:
        body = {"model": self.model, "messages": messages}
        optional = {
            "temperature": self.temperature,
            "top_p": self.top_p,
            "max_completion_tokens": self.max_tokens,
            "presence_penalty": self.presence_penalty,
            "service_tier": self.service_tier,
            "reasoning_effort": self.reasoning_effort,
            "verbosity": self.verbosity,
        }
        body.update({k: v for k, v in optional.items() if v is not None})
        return body

    def describe(self) -> dict:
        d = dict(self.__dict__)
        d.pop("api_key")
        return d


@dataclass
class Completion:
    text: str
    finish_reason: str | None
    error: str | None = None


class ResponseCache:
    """Responses keyed by the exact request body. With temperature 0 a
    repeated request is redundant; GEPA re-evaluates candidates often."""

    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self._db = sqlite3.connect(path, check_same_thread=False)
        self._db.execute("CREATE TABLE IF NOT EXISTS responses (key TEXT PRIMARY KEY, value TEXT)")
        self._lock = threading.Lock()

    @staticmethod
    def key(body: dict) -> str:
        return hashlib.sha256(json.dumps(body, sort_keys=True, ensure_ascii=False).encode()).hexdigest()

    def get(self, key: str) -> Completion | None:
        with self._lock:
            row = self._db.execute("SELECT value FROM responses WHERE key = ?", (key,)).fetchone()
        return Completion(**json.loads(row[0])) if row else None

    def put(self, key: str, c: Completion) -> None:
        with self._lock:
            self._db.execute("INSERT OR REPLACE INTO responses VALUES (?, ?)", (key, json.dumps(c.__dict__)))
            self._db.commit()


class Translator:
    def __init__(self, params: ChatParams, cache: ResponseCache, concurrency: int):
        self.params = params
        self.cache = cache
        self.concurrency = concurrency

    async def _one(self, client: httpx.AsyncClient, sem: asyncio.Semaphore, messages: list[dict]) -> Completion:
        body = self.params.body(messages)
        key = ResponseCache.key(body)
        if (hit := self.cache.get(key)) is not None:
            return hit
        headers = {"Authorization": f"Bearer {self.params.api_key}"} if self.params.api_key else {}
        async with sem:
            for attempt in range(3):
                try:
                    r = await client.post(f"{self.params.endpoint}/v1/chat/completions", json=body, headers=headers)
                    r.raise_for_status()
                    choice = r.json()["choices"][0]
                    text = choice["message"].get("content") or ""
                    # niinii's streaming path drops newlines from each delta.
                    if self.params.stream:
                        text = text.replace("\n", "")
                    c = Completion(text=text, finish_reason=choice.get("finish_reason"))
                    self.cache.put(key, c)
                    return c
                except (httpx.HTTPError, KeyError, ValueError) as e:
                    err = f"{type(e).__name__}: {e}"
                    await asyncio.sleep(2 * (attempt + 1))
            return Completion(text="", finish_reason=None, error=err)

    def translate(self, prompts: list[list[dict]]) -> list[Completion]:
        async def run():
            sem = asyncio.Semaphore(self.concurrency)
            async with httpx.AsyncClient(timeout=self.params.timeout_s) as client:
                return await asyncio.gather(*(self._one(client, sem, m) for m in prompts))

        return asyncio.run(run())
