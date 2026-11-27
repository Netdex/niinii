"""The chat translator's request layout, mirrored from niinii.

Keep in sync with `build_prompt` and `LINE_PREFIX` in
niinii/src/translator/chat.rs, and `build_prompt` / `VNDB_PROMPT_PREAMBLE`
in niinii/src/vndb.rs.
"""

from __future__ import annotations

import json
import time
from pathlib import Path

import httpx

LINE_PREFIX = "Translate into English:\n"
VNDB_API = "https://api.vndb.org/kana"
VNDB_PROMPT_PREAMBLE = (
    "Use the following character list for names, genders and speakers. When the speaker needs to be "
    "inferred, try your best using the character information provided. If you are unsure of "
    "the speaker's gender, use neutral pronouns such as 'singular -they'."
)
SEX_LABEL = {"m": "male", "f": "female", "b": "both", "n": "sexless"}


def build_messages(
    system_prompt: str, addendum: str | None, context: list[list[str]], line: str
) -> list[dict]:
    """One system message (prompt + addendum), each earlier line as the
    user/assistant turn that translated it, then the line to translate. Every
    user line carries LINE_PREFIX."""
    system = system_prompt
    if addendum:
        if system:
            system += "\n\n"
        system += addendum
    messages = [{"role": "system", "content": system}]
    for ja, en in context:
        messages.append({"role": "user", "content": LINE_PREFIX + ja})
        messages.append({"role": "assistant", "content": en})
    messages.append({"role": "user", "content": LINE_PREFIX + line})
    return messages


# -- VNDB addendum ---------------------------------------------------------


def _post(client: httpx.Client, path: str, body: dict) -> dict:
    for attempt in range(5):
        r = client.post(VNDB_API + path, json=body)
        if r.status_code == 429:
            time.sleep(30 * (attempt + 1))
            continue
        r.raise_for_status()
        return r.json()
    raise RuntimeError(f"VNDB rate limit persisted for {path}")


def fetch_vn(vn_id: str, cache_dir: Path) -> dict:
    """VN summary and all characters, as the app requests them. Cached as
    raw API results so the addendum can be rebuilt offline."""
    path = cache_dir / f"{vn_id}.json"
    if path.exists():
        return json.loads(path.read_text(encoding="utf-8"))
    with httpx.Client(timeout=30, headers={"User-Agent": "niinii-prompt-tune/0.1"}) as client:
        vn = _post(client, "/vn", {"filters": ["id", "=", vn_id], "fields": "title,alttitle", "results": 1})
        if not vn["results"]:
            raise ValueError(f"unknown VNDB id {vn_id}")
        characters, page = [], 1
        while True:
            resp = _post(client, "/character", {
                "filters": ["vn", "=", ["id", "=", vn_id]],
                "fields": "name,original,aliases,sex,vns.id,vns.role",
                "results": 100,
                "page": page,
            })
            characters += resp["results"]
            if not resp.get("more"):
                break
            page += 1
    data = {"vn": vn["results"][0], "characters": characters}
    cache_dir.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    return data


def vndb_addendum(data: dict) -> str:
    vn = data["vn"]
    out = [VNDB_PROMPT_PREAMBLE, "\n\nVisual novel: ", vn["title"]]
    if vn.get("alttitle"):
        out.append(f" ({vn['alttitle']})")
    out.append("\n\nCharacters:\n")
    for ch in data["characters"]:
        line = "- "
        line += f"{ch['original']} ({ch['name']})" if ch.get("original") else ch["name"]
        sex = (ch.get("sex") or [None])[0]
        if sex:
            line += f" [{SEX_LABEL[sex]}]"
        role = next((v.get("role") for v in ch.get("vns", []) if v["id"] == vn["id"]), None)
        if role:
            line += f" [{role}]"
        aliases = [a.strip() for a in ch.get("aliases") or [] if a.strip()]
        if aliases:
            line += f" (aka: {'; '.join(aliases)})"
        out.append(line + "\n")
    return "".join(out)
