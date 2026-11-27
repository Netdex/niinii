"""Scoring: chrF++ against the VNTL reference, minus penalties for failure
modes seen with local models in niinii's chat backend. Each penalty comes
with feedback text that the reflection model reads."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from sacrebleu.metrics import CHRF

from .dataset import Case, normalize_en

CHRF_PP = CHRF(word_order=2)

JAPANESE_RE = re.compile(r"[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uff66-\uff9f]")
# Echoed prompt text: the line prefix, or VNTL-style markers.
TAG_RE = re.compile(r"(?i)translate into english:|</s>|<<[A-Z]+>>")
EXTRA_RE = re.compile(
    r"(?i)\b(translator'?s? note|translation|literally|romaji|note)\s*:|^\s*(here is|sure[,!])"
)
END_PUNCT = tuple(".!?\"')]~*-") + ("\u2014", "\u2013")

PENALTY = {
    "tags": 0.2,
    "extra": 0.2,
    "translated_context": 0.3,
    "cut_off": 0.15,
    "speaker_missing": 0.2,
    "wrong_speaker": 0.3,
    "invented_speaker": 0.3,
    "names": 0.1,
}
# Failures that make the output unusable score 0 regardless of overlap.
FATAL = ("error", "japanese")

# A speaker prefix on normalized output: up to three capitalized words, then
# ":" or a space, then the quoted line. The separator style isn't judged.
PREFIX_RE = re.compile(r"^(?P<name>[A-Z][\w'.-]*(?: [A-Z][\w'.-]*){0,2})(?::\s*|\s)(?=[\"(])")


@dataclass
class Score:
    score: float
    chrf: float
    issues: dict[str, str] = field(default_factory=dict)

    def feedback(self) -> str:
        head = f"Score {self.score:.3f} (chrF++ of the translated text vs reference {self.chrf:.3f})."
        if not self.issues:
            return head + " No rule violations; remaining gap is wording vs the reference."
        return head + " Problems:\n" + "\n".join(f"- {v}" for v in self.issues.values())


def chrf(hyp: str, ref: str) -> float:
    return CHRF_PP.sentence_score(hyp, [ref]).score / 100


def fold(s: str) -> str:
    """Romanization-insensitive form for name matching (Yuuma/Yuma, Ou/O)."""
    s = s.lower().replace("\u014d", "o").replace("\u016b", "u")
    return re.sub(r"(?<=[aeiou])[uh](?![aeiou])|(?<=o)u", "", s)


def split_speaker(case: Case, hyp: str) -> tuple[str | None, str, str]:
    """(output speaker prefix, output text, reference text). Speaker prefixes
    are scored separately, so chrF compares only the translated text."""
    m = PREFIX_RE.match(hyp)
    out_name, out_body = (m.group("name"), hyp[m.end() :]) if m else (None, hyp)
    ref = case.reference
    if case.speaker and ref.startswith(case.speaker + ": "):
        ref = ref[len(case.speaker) + 2 :]
    return out_name, out_body, ref


def speaker_variants(case: Case) -> list[str] | None:
    """Accepted English forms of the speaker's name, or None when the speaker
    is not a listed character (e.g. "Taxi Driver"), whose wording is free."""
    tokens = {fold(t) for t in case.speaker.split()}
    for variants in case.names:
        if any(fold(v) in tokens or fold(v) == fold(case.speaker) for v in variants):
            return variants
    return None


def score(case: Case, output: str, finish_reason: str | None, error: str | None) -> Score:
    if error:
        return Score(0.0, 0.0, {"error": f"Request failed: {error}"})
    hyp = normalize_en(output)
    out_name, out_body, ref_body = split_speaker(case, hyp)
    base = chrf(out_body, ref_body) if out_body else 0.0
    issues: dict[str, str] = {}

    if JAPANESE_RE.search(hyp):
        issues["japanese"] = (
            "Output still contains Japanese text (often the speaker name); everything must be rendered in English."
        )
    if case.speaker:
        accepted = speaker_variants(case)
        # Placeholder speakers such as "???" can't be matched as a prefix.
        if out_name is None and case.speaker[:1].isalpha() and not JAPANESE_RE.search(hyp[:12]):
            issues["speaker_missing"] = (
                f"Dialogue line lost its speaker; the reader can't tell who is talking. Expected {case.speaker}."
            )
        elif out_name and accepted and not any(fold(v) in fold(out_name) for v in accepted):
            issues["wrong_speaker"] = f"Line attributed to {out_name}; the speaker is {case.speaker}."
    elif out_name:
        issues["invented_speaker"] = (
            f"Narration (no speaker in the source) was turned into dialogue attributed to {out_name}."
        )
    if TAG_RE.search(output):
        issues["tags"] = "Output echoes prompt text or markup (e.g. 'Translate into English:'); reply with the translation only."
    long = len(hyp) > 1.8 * len(case.reference) + 40
    if EXTRA_RE.search(hyp) or (long and "(" in hyp and "(" not in case.reference):
        issues["extra"] = "Output adds commentary (notes, explanations, alternatives) beyond the translation."
    prev = max((chrf(hyp, p) for p in case.prev_references), default=0.0)
    if hyp and prev > 0.45 and prev > chrf(hyp, case.reference) + 0.1:
        issues["translated_context"] = (
            "Output translates an earlier line instead of (or in addition to) the final message."
        )
    elif long and case.prev_references and len(hyp) > len(case.reference) + 0.8 * len(case.prev_references[-1]):
        issues["translated_context"] = "Output is far longer than the line; it likely translates several lines."
    if finish_reason == "length":
        issues["cut_off"] = "Output hit the token limit and was cut off."
    elif case.reference.endswith(END_PUNCT) and hyp and not hyp.endswith(END_PUNCT):
        issues["cut_off"] = "Output ends mid-sentence or without closing punctuation; the translation must be complete."
    elif hyp and len(hyp) < 0.4 * len(case.reference):
        issues["cut_off"] = "Output is much shorter than the source warrants; content was dropped."
    # Names inside the line itself; the speaker is judged above.
    folded = fold(out_body)
    ref_folded = fold(ref_body)
    missing = [
        v[0]
        for v in case.names
        if any(fold(n) in ref_folded for n in v) and not any(fold(n) in folded for n in v)
    ]
    if missing:
        issues["names"] = "Names missing or misread in the line: expected " + ", ".join(missing) + "."

    if any(k in issues for k in FATAL):
        return Score(0.0, base, issues)
    penalty = sum(PENALTY[k] for k in issues)
    return Score(max(0.0, base - penalty), base, issues)
