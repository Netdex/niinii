"""LLM judge for translations, run through Claude Code headless.

chrF against one reference can't tell a valid paraphrase from a wrong
translation; the judge rates meaning and fluency against the source, using
the reference only as a guide to the intended meaning. Lines are judged in
batches, blind to which prompt produced them, and cached by (case, output).
"""

from __future__ import annotations

import hashlib
import json
import sqlite3
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

from .dataset import Case
from .reflection import ClaudeCodeLM

# Bump when the rubric or prompt changes so cached judgments are not reused.
JUDGE_VERSION = 1

SYSTEM_PROMPT = (
    "You are an expert Japanese-to-English translation reviewer for visual novels. "
    "You grade translations strictly and consistently, and answer only with the requested JSON."
)

RUBRIC = """Grade each candidate English translation of a Japanese visual novel line.

For each item you get the preceding script lines (Japanese, for context), the line to translate, a reference translation by a professional, and the candidate. The reference shows the intended meaning; the candidate does not need to match its wording, and a different but accurate and natural rendering deserves full marks.

Score:
- meaning (1-5): 5 = everything in the source is conveyed accurately; 4 = minor nuance lost or slightly off; 3 = partly wrong or an important element missing or added; 2 = mostly wrong; 1 = unrelated, untranslated, or translates a different line.
- fluency (1-5): 5 = reads like natural English writing; 3 = understandable but stiff or awkward; 1 = broken.
- speaker_ok: for dialogue (the source starts with a speaker name), true if the candidate names the right speaker in English in any form; for narration, true unless the candidate invents a speaker. Ignore separator style ("Name: " vs "Name  ").
- errors: short labels for concrete problems, e.g. "mistranslation: <what>", "omission: <what>", "addition", "wrong speaker", "untranslated Japanese", "wrong subject/pronoun", "tense", "translates context line". Empty list if none.

Answer with a JSON array, one object per item, in order:
[{"id": "...", "meaning": 1-5, "fluency": 1-5, "speaker_ok": true, "errors": []}, ...]

Items:
"""


@dataclass
class Judgment:
    meaning: int
    fluency: int
    speaker_ok: bool
    errors: list[str] = field(default_factory=list)

    @property
    def score(self) -> float:
        """0..1, weighted toward meaning."""
        return 0.7 * (self.meaning - 1) / 4 + 0.3 * (self.fluency - 1) / 4


class JudgeCache:
    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self._db = sqlite3.connect(path, check_same_thread=False)
        self._db.execute("CREATE TABLE IF NOT EXISTS judgments (key TEXT PRIMARY KEY, value TEXT)")

    @staticmethod
    def key(case: Case, output: str) -> str:
        raw = json.dumps([JUDGE_VERSION, case.id, case.line, output], ensure_ascii=False)
        return hashlib.sha256(raw.encode()).hexdigest()

    def get(self, key: str) -> Judgment | None:
        row = self._db.execute("SELECT value FROM judgments WHERE key = ?", (key,)).fetchone()
        return Judgment(**json.loads(row[0])) if row else None

    def put(self, key: str, j: Judgment) -> None:
        self._db.execute("INSERT OR REPLACE INTO judgments VALUES (?, ?)", (key, json.dumps(j.__dict__)))
        self._db.commit()


def _item(i: int, case: Case, output: str) -> str:
    context = "\n".join(ja for ja, _ in case.context[-5:]) or "(none)"
    return (
        f"--- item {i}\nPreceding lines:\n{context}\nLine to translate:\n{case.line}\n"
        f"Reference:\n{case.reference}\nCandidate:\n{output or '(empty)'}\n"
    )


def _parse(text: str, n: int) -> list[Judgment]:
    start, end = text.find("["), text.rfind("]")
    rows = json.loads(text[start : end + 1])
    if len(rows) != n:
        raise ValueError(f"judge returned {len(rows)} items for {n}")
    return [
        Judgment(int(r["meaning"]), int(r["fluency"]), bool(r["speaker_ok"]), [str(e) for e in r.get("errors", [])])
        for r in rows
    ]


class Judge:
    def __init__(self, cache: JudgeCache, log_dir: Path, model: str | None = None, batch: int = 20, parallel: int = 4):
        self.cache = cache
        self.lm = ClaudeCodeLM(model, None, log_dir, system_prompt=SYSTEM_PROMPT, log_prefix="judge")
        self.batch = batch
        self.parallel = parallel

    def _judge_batch(self, items: list[tuple[Case, str]]) -> list[Judgment]:
        prompt = RUBRIC + "".join(_item(i, c, o) for i, (c, o) in enumerate(items))
        last = None
        for _ in range(2):
            try:
                return _parse(self.lm(prompt), len(items))
            except (ValueError, KeyError, TypeError, json.JSONDecodeError) as e:
                last = e
        raise RuntimeError(f"judge output unusable: {last}")

    def judge(self, items: list[tuple[Case, str]]) -> list[Judgment]:
        keys = [JudgeCache.key(c, o) for c, o in items]
        results: list[Judgment | None] = [self.cache.get(k) for k in keys]
        todo = [i for i, r in enumerate(results) if r is None]
        batches = [todo[i : i + self.batch] for i in range(0, len(todo), self.batch)]
        with ThreadPoolExecutor(self.parallel) as pool:
            for idxs, judged in zip(batches, pool.map(lambda b: self._judge_batch([items[i] for i in b]), batches)):
                for i, j in zip(idxs, judged):
                    results[i] = j
                    self.cache.put(keys[i], j)
        return results  # type: ignore[return-value]
