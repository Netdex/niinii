"""VNTL parsing and evaluation-case construction.

A VNTL row is a ~1k-token chunk of one VN's script: a <<METADATA>> block of
characters, then <<JAPANESE>>/<<ENGLISH>> pairs. The last pair is often cut
off by the chunking, so only pairs whose English ends in </s> are used.
"""

from __future__ import annotations

import json
import random
import re
from collections import Counter
from dataclasses import asdict, dataclass, field
from pathlib import Path

import pyarrow.parquet as pq

DATASET = "lmg-anon/VNTL-v3.1-1k"
SPLITS = ("train", "val")
PARQUET = {
    "train": "data/train-00000-of-00001-eb879b20cbd4854b.parquet",
    "val": "data/val-00000-of-00001-51ab569c62b2bbc8.parquet",
}

CHAR_RE = re.compile(r"^\[character\] Name: (?P<en>.*?) \((?P<ja>[^)]*)\)(?P<rest>.*)$")
ALIAS_RE = re.compile(r"(?P<en>[^,()]+?) \((?P<ja>[^)]*)\)")
PAIR_RE = re.compile(r"<<JAPANESE>>\n(?P<ja>.*?)\n<<ENGLISH>>\n(?P<en>.*?)(?=\n<<JAPANESE>>|\Z)", re.S)
SPEAKER_RE = re.compile(r"^\[(?P<speaker>[^\]]+)\]: (?P<body>.*)$", re.S)
# Protagonist names the player can change are templated in the source.
PLACEHOLDER_NAMES = {"%name"}


@dataclass
class Character:
    en: str
    ja: str
    gender: str | None = None
    aliases: list[tuple[str, str]] = field(default_factory=list)


@dataclass
class Pair:
    ja: str
    en: str


@dataclass
class Row:
    split: str
    index: int
    characters: list[Character]
    pairs: list[Pair]


@dataclass
class Case:
    id: str
    vn_id: str | None
    vn_title: str | None
    # Earlier (Japanese line, translation) turns the app would send.
    context: list[list[str]]
    line: str
    reference: str
    prev_references: list[str]
    speaker: str | None
    names: list[list[str]]

    def to_json(self) -> dict:
        return asdict(self)


def download(split: str, data_dir: Path) -> Path:
    import httpx

    path = data_dir / f"vntl-{split}.parquet"
    if not path.exists():
        url = f"https://huggingface.co/datasets/{DATASET}/resolve/main/{PARQUET[split]}"
        data_dir.mkdir(parents=True, exist_ok=True)
        with httpx.stream("GET", url, follow_redirects=True, timeout=120) as r:
            r.raise_for_status()
            with open(path, "wb") as f:
                for chunk in r.iter_bytes():
                    f.write(chunk)
    return path


def parse_row(split: str, index: int, text: str) -> Row:
    head, _, body = text.partition("<<START>>")
    characters = []
    for line in head.splitlines():
        m = CHAR_RE.match(line.strip())
        if not m:
            continue
        gender = None
        aliases = []
        for part in m.group("rest").split("|"):
            key, _, value = part.strip().partition(": ")
            if key == "Gender":
                gender = value.strip()
            elif key == "Aliases":
                aliases = [(a.group("en").strip(), a.group("ja")) for a in ALIAS_RE.finditer(value)]
        characters.append(Character(m.group("en").strip(), m.group("ja").strip(), gender, aliases))
    pairs = []
    for m in PAIR_RE.finditer(body.strip("\n")):
        en = m.group("en")
        if not en.endswith("</s>"):
            break
        pairs.append(Pair(m.group("ja").strip(), en[: -len("</s>")].strip()))
    return Row(split, index, characters, pairs)


def load_rows(split: str, data_dir: Path) -> list[Row]:
    table = pq.read_table(download(split, data_dir)).to_pylist()
    return [parse_row(split, i, r["text"]) for i, r in enumerate(table)]


def to_niinii_line(ja: str) -> str:
    """Render a VNTL Japanese line the way a text hooker typically delivers it:
    `name  \u300c...\u300d` for dialogue, bare text for narration."""
    m = SPEAKER_RE.match(ja)
    if not m:
        return ja
    return f"{m.group('speaker')}  {m.group('body')}"


QUOTE_MAP = str.maketrans(
    {
        "\u300c": '"', "\u300d": '"', "\u300e": '"', "\u300f": '"',
        "\u201c": '"', "\u201d": '"', "\u2018": "'", "\u2019": "'",
        "\u2026": "...", "\uff01": "!", "\uff1f": "?", "\u3000": " ",
    }
)


def normalize_en(s: str) -> str:
    """Canonical English form for scoring: `[Speaker]: \u300cx\u300d` becomes
    `Speaker: "x"`, typographic quotes and ellipses become ASCII."""
    s = s.strip()
    m = SPEAKER_RE.match(s)
    if m:
        s = f"{m.group('speaker')}: {m.group('body')}"
    s = s.translate(QUOTE_MAP)
    return re.sub(r"\s+", " ", s).strip()


def speaker_of(en: str) -> str | None:
    m = SPEAKER_RE.match(en.strip())
    return m.group("speaker") if m else None


def name_variants(c: Character) -> list[str]:
    """English forms of a character's name that may appear in a translation."""
    out = [c.en, *c.en.split()]
    out += [a for a, _ in c.aliases]
    return [v for v in dict.fromkeys(out) if len(v) >= 2]


def build_case(
    row: Row, target: int, vn: tuple[str, str] | None, context: list[list[str]], prev_references: list[str]
) -> Case:
    pair = row.pairs[target]
    reference = normalize_en(pair.en)
    # Names the reference uses; a translation should carry the same names.
    names = []
    for c in row.characters:
        if c.ja in PLACEHOLDER_NAMES:
            continue
        variants = name_variants(c)
        if any(re.search(rf"\b{re.escape(v)}\b", reference) for v in variants):
            names.append(variants)
    return Case(
        id=f"{row.split}-{row.index}-{target}",
        vn_id=vn[0] if vn else None,
        vn_title=vn[1] if vn else None,
        context=context,
        line=to_niinii_line(pair.ja),
        reference=reference,
        prev_references=prev_references,
        speaker=speaker_of(pair.en),
        names=names,
    )


def load_vn_map(path: Path) -> tuple[dict[str, str], dict[str, list[str]]]:
    """`vn_map.json`: VNTL rows carry no VN id, so each Japanese character
    name maps to the VNDB ids of the VNs it appears in."""
    raw = json.loads(path.read_text(encoding="utf-8"))
    return raw["vns"], raw["characters"]


def row_vn(row: Row, characters: dict[str, list[str]]) -> str | None:
    """The VN most of the row's characters belong to; ties go to the lowest
    id, i.e. the original over a sequel sharing the cast."""
    votes = Counter(vn for c in row.characters for vn in characters.get(c.ja, []))
    if not votes:
        return None
    best = max(votes.values())
    return min((vn for vn, n in votes.items() if n == best), key=lambda vn: int(vn[1:]))


def streams(rows: list[Row]) -> list[list[tuple[Row, int]]]:
    """Chain consecutive rows (adjacent indices in one split) into continuous
    script streams of (row, pair index)."""
    out: list[list[tuple[Row, int]]] = []
    prev = None
    for r in sorted(rows, key=lambda r: (r.split, r.index)):
        if prev is None or r.split != prev.split or r.index != prev.index + 1:
            out.append([])
        out[-1] += [(r, i) for i in range(len(r.pairs))]
        prev = r
    return out


_ENCODING = None


def estimate_tokens(text: str) -> int:
    """niinii's `Message::estimate_tokens`: cl100k tokens plus 4 per message."""
    global _ENCODING
    if _ENCODING is None:
        import tiktoken

        _ENCODING = tiktoken.get_encoding("cl100k_base")
    return 4 + len(_ENCODING.encode(text, disallowed_special=()))


def replay(stream: list[tuple[Row, int]], limits: tuple[int, int]) -> list[tuple[list[list[str]], bool]]:
    """The context the app would hold before translating each line of the
    stream, and whether its buffer has been trimmed yet (steady state).

    Mirrors the chat backend: after each line the user line and the reply are
    appended, and `enforce_context_limit` trims oldest turns down to
    `limits[0]` once the total exceeds `limits[1]`. The VNTL reference stands
    in for the model's reply, which the app also counts toward the limit."""
    lo, hi = limits
    buffer: list[tuple[str, str, int]] = []  # (role, content, tokens)
    total, trimmed = 0, False
    out = []
    for row, i in stream:
        turns = [c for _, c, _ in buffer]
        out.append(([turns[k : k + 2] for k in range(0, len(turns), 2)], trimmed))
        pair = row.pairs[i]
        for role, content in (("user", to_niinii_line(pair.ja)), ("assistant", normalize_en(pair.en))):
            t = estimate_tokens(content)
            buffer.append((role, content, t))
            total += t
        if total > hi:
            trimmed = True
            while total > lo and buffer:
                total -= buffer.pop(0)[2]
                while buffer and buffer[0][0] != "user":
                    total -= buffer.pop(0)[2]
    return out


def split_cases(
    rows_by_vn: dict[str, list[Row]],
    titles: dict[str, str],
    sizes: dict[str, int],
    test_vn_fraction: float,
    context_limits: tuple[int, int],
    seed: int,
) -> dict[str, list[Case]]:
    """Cases at random points in each VN's script, with the context the app
    would hold there after translating everything before it. Only points past
    the buffer's first trim are used, so every case has a full-length history.
    Test VNs are disjoint from train/val VNs so the test score measures
    whether a prompt carries over to unseen VNs."""
    rng = random.Random(seed)
    vns = sorted(rows_by_vn, key=lambda vn: int(vn[1:]))
    rng.shuffle(vns)
    n_test = max(1, round(len(vns) * test_vn_fraction))

    def candidates(vn: str) -> list[Case]:
        out = []
        for stream in streams(rows_by_vn[vn]):
            contexts = replay(stream, context_limits)
            refs = [normalize_en(row.pairs[i].en) for row, i in stream]
            for pos, ((row, i), (context, steady)) in enumerate(zip(stream, contexts)):
                if steady:
                    out.append((row, i, context, refs[max(0, pos - 3) : pos]))
        rng.shuffle(out)
        return out

    def sample(vn_ids: list[str], n: int) -> list[Case]:
        # Round-robin across VNs so one long VN doesn't dominate.
        pools = {vn: candidates(vn) for vn in vn_ids}
        out = []
        while len(out) < n and any(pools.values()):
            for vn, pool in pools.items():
                if pool and len(out) < n:
                    row, i, context, prev = pool.pop()
                    out.append(build_case(row, i, (vn, titles[vn]), context, prev))
        return out

    tune = sample(vns[n_test:], sizes["train"] + sizes["val"])
    rng.shuffle(tune)
    return {
        "train": tune[: sizes["train"]],
        "val": tune[sizes["train"] :],
        "test": sample(vns[:n_test], sizes["test"]),
    }
