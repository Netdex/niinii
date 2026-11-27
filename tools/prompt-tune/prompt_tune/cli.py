"""prompt-tune: tune niinii's chat system prompt with GEPA on VNTL.

  prepare   download VNTL, sample cases, fetch VNDB character lists
  eval      score one system prompt on a split
  tune      run GEPA on train/val, then score seed and best prompt on test
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
import tomllib
from collections import Counter
from pathlib import Path

from .dataset import SPLITS, Case, load_rows, load_vn_map, row_vn, split_cases
from .render import fetch_vn, vndb_addendum
from .task import ChatParams, ResponseCache, Translator, find_config

TOOL_DIR = Path(__file__).resolve().parent.parent
REPO_ROOT = TOOL_DIR.parent.parent
DATA = TOOL_DIR / "data"
RUNS = TOOL_DIR / "runs"


def log(msg: str) -> None:
    print(msg, file=sys.stderr, flush=True)


def load_config(args) -> tuple[dict, Path | None]:
    """niinii.toml, which supplies the [chat] request settings, context limits
    and system prompt the app runs with."""
    path = Path(args.config) if args.config else find_config(REPO_ROOT)
    return (tomllib.loads(path.read_text(encoding="utf-8")) if path else {}), path


# -- prepared data -----------------------------------------------------------


def load_cases() -> dict[str, list[Case]]:
    path = DATA / "cases.json"
    if not path.exists():
        sys.exit("no prepared cases; run `prompt-tune prepare` first")
    raw = json.loads(path.read_text(encoding="utf-8"))
    return {split: [Case(**c) for c in cases] for split, cases in raw.items()}


def load_addenda() -> dict[str, str]:
    return json.loads((DATA / "addenda.json").read_text(encoding="utf-8"))


def cmd_prepare(args) -> None:
    titles, characters = load_vn_map(TOOL_DIR / "vn_map.json")
    rows = [r for split in SPLITS for r in load_rows(split, DATA)]
    rows_by_vn: dict[str, list] = {}
    unmatched = 0
    for r in rows:
        vn = row_vn(r, characters)
        if vn is None:
            unmatched += 1
        else:
            rows_by_vn.setdefault(vn, []).append(r)
    log(f"{len(rows)} VNTL rows in {len(rows_by_vn)} VNs ({unmatched} unmatched rows skipped)")

    config, config_path = load_config(args)
    limits = tuple(config.get("chat", {}).get("max_context_tokens", [1024, 2048]))
    log(f"context limits {list(limits)} (from {config_path or 'defaults'})")
    sizes = {"train": args.train, "val": args.val, "test": args.test}
    cases = split_cases(rows_by_vn, titles, sizes, args.test_vn_fraction, limits, args.seed)
    for split, cs in cases.items():
        vns = Counter(c.vn_title for c in cs)
        lens = sorted(len(c.context) for c in cs)
        log(f"{split}: {len(cs)} cases, context turns {lens[0]}-{lens[-1]} (median {lens[len(lens) // 2]}), from "
            + ", ".join(f"{t} ({n})" for t, n in vns.most_common()))
    DATA.mkdir(parents=True, exist_ok=True)
    (DATA / "cases.json").write_text(
        json.dumps({k: [c.to_json() for c in v] for k, v in cases.items()}, ensure_ascii=False, indent=1),
        encoding="utf-8",
    )

    vn_ids = sorted({c.vn_id for split in cases.values() for c in split if c.vn_id})
    addenda = {vn_id: vndb_addendum(fetch_vn(vn_id, DATA / "vndb")) for vn_id in vn_ids}
    (DATA / "addenda.json").write_text(json.dumps(addenda, ensure_ascii=False, indent=1), encoding="utf-8")


# -- evaluation ----------------------------------------------------------------


def make_translator(args) -> tuple[Translator, dict]:
    config, config_path = load_config(args)
    params = ChatParams.from_config(config, args.endpoint, args.model)
    if not params.endpoint or not params.model:
        sys.exit("no endpoint/model: pass --endpoint and --model")
    log(f"translator: {params.model} @ {params.endpoint} (chat settings from {config_path or 'defaults'})")
    return Translator(params, ResponseCache(DATA / "responses.sqlite"), args.concurrency), config


def system_prompt(args, config: dict) -> str:
    if args.prompt:
        return Path(args.prompt).read_text(encoding="utf-8").strip()
    return config.get("chat", {}).get("system_prompt", "")


def summarize(rollouts) -> dict:
    issues = Counter(k for r in rollouts for k in r.result.issues)
    scores = [r.result.score for r in rollouts]
    out = {
        "n": len(scores),
        "score": statistics.fmean(scores),
        "score_stderr": statistics.stdev(scores) / len(scores) ** 0.5 if len(scores) > 1 else 0.0,
        "chrf": statistics.fmean(r.result.chrf for r in rollouts),
        "issues": dict(issues.most_common()),
    }
    js = [r.judgment for r in rollouts if r.judgment is not None]
    if js:
        judge_scores = [j.score for j in js]
        out["judge"] = {
            "score": statistics.fmean(judge_scores),
            "score_stderr": statistics.stdev(judge_scores) / len(js) ** 0.5 if len(js) > 1 else 0.0,
            "meaning": statistics.fmean(j.meaning for j in js),
            "fluency": statistics.fmean(j.fluency for j in js),
            "speaker_ok": sum(j.speaker_ok for j in js) / len(js),
            "errors": dict(Counter(e.split(":")[0].strip().lower() for j in js for e in j.errors).most_common()),
        }
    return out


def judge_rollouts(rollouts, args, log_dir: Path) -> None:
    from .judge import Judge, JudgeCache

    judge = Judge(JudgeCache(DATA / "judgments.sqlite"), log_dir, model=args.judge_model)
    for r, j in zip(rollouts, judge.judge([(r.case, r.output) for r in rollouts])):
        r.judgment = j
    log(f"judge: {judge.lm.calls} Claude Code calls, ${judge.lm.total_cost:.2f}")


def write_rollouts(path: Path, rollouts) -> None:
    with open(path, "w", encoding="utf-8") as f:
        for r in rollouts:
            row = {
                "id": r.case.id, "vn": r.case.vn_title, "line": r.case.line, "reference": r.case.reference,
                "output": r.output, "score": r.result.score, "chrf": r.result.chrf, "issues": r.result.issues,
            }
            if r.judgment is not None:
                row["judge"] = {**r.judgment.__dict__, "score": r.judgment.score}
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


def cmd_eval(args) -> None:
    from .adapter import evaluate_prompt

    cases = load_cases()[args.split]
    translator, config = make_translator(args)
    rollouts = evaluate_prompt(cases, load_addenda(), translator, system_prompt(args, config))
    if args.judge:
        judge_rollouts(rollouts, args, DATA / "judge-logs")
    out = Path(args.out) if args.out else DATA / f"eval-{args.split}.jsonl"
    write_rollouts(out, rollouts)
    print(json.dumps(summarize(rollouts), indent=1))
    log(f"per-case results: {out}")


# -- tuning --------------------------------------------------------------------


class RunLogger:
    """GEPA logger writing UTF-8 to run_log.txt and stderr. GEPA's default
    file logger uses the locale encoding, which fails on Japanese text."""

    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self.file = open(path, "a", encoding="utf-8")

    def log(self, message: str) -> None:
        self.file.write(message + "\n")
        self.file.flush()
        sys.stderr.buffer.write((message + "\n").encode("utf-8", "replace"))
        sys.stderr.flush()


def cmd_tune(args) -> None:
    import gepa

    from .adapter import COMPONENT, NiiniiAdapter, evaluate_prompt
    from .reflection import REFLECTION_TEMPLATE, ClaudeCodeLM

    cases = load_cases()
    addenda = load_addenda()
    translator, config = make_translator(args)
    seed = system_prompt(args, config)
    run_dir = RUNS / (args.name or time.strftime("%Y%m%d-%H%M%S"))
    run_dir.mkdir(parents=True, exist_ok=True)
    (run_dir / "seed_prompt.txt").write_text(seed, encoding="utf-8")
    run = {
        "translator": translator.params.describe(),
        "sizes": {k: len(v) for k, v in cases.items()},
        **{k: getattr(args, k) for k in ("max_metric_calls", "minibatch", "reflection_model",
                                         "reflection_effort", "max_reflection_cost")},
    }
    (run_dir / "run.json").write_text(json.dumps(run, indent=1), encoding="utf-8")

    reflection = ClaudeCodeLM(args.reflection_model, args.reflection_effort, run_dir / "reflections")
    result = gepa.optimize(
        seed_candidate={COMPONENT: seed},
        trainset=cases["train"],
        valset=cases["val"],
        adapter=NiiniiAdapter(addenda, translator),
        reflection_lm=reflection,
        reflection_prompt_template=REFLECTION_TEMPLATE,
        reflection_minibatch_size=args.minibatch,
        max_metric_calls=args.max_metric_calls,
        max_reflection_cost=args.max_reflection_cost,
        run_dir=str(run_dir / "gepa"),
        logger=RunLogger(run_dir / "gepa" / "run_log.txt"),
        seed=args.seed,
        display_progress_bar=True,
    )
    # GEPA logs proposer exceptions and keeps going, which would otherwise
    # spend the whole budget without proposing anything.
    if reflection.calls == 0:
        sys.exit(f"no reflection call succeeded; see the GEPA log above. Run dir: {run_dir}")
    best = result.best_candidate[COMPONENT]
    (run_dir / "best_prompt.txt").write_text(best, encoding="utf-8")
    candidates = [
        {"idx": i, "val_score": s, "system_prompt": c[COMPONENT]}
        for i, (c, s) in enumerate(zip(result.candidates, result.val_aggregate_scores))
    ]
    (run_dir / "candidates.json").write_text(json.dumps(candidates, ensure_ascii=False, indent=1), encoding="utf-8")

    log("scoring seed and best prompt on the held-out test split")
    seed_test = evaluate_prompt(cases["test"], addenda, translator, seed)
    best_test = evaluate_prompt(cases["test"], addenda, translator, best)
    if args.judge:
        judge_rollouts(seed_test + best_test, args, run_dir / "judge-logs")
    write_rollouts(run_dir / "test-seed.jsonl", seed_test)
    write_rollouts(run_dir / "test-best.jsonl", best_test)
    report = {
        "val": {"seed": result.val_aggregate_scores[0], "best": result.val_aggregate_scores[result.best_idx]},
        "test": {"seed": summarize(seed_test), "best": summarize(best_test)},
        "candidates": len(candidates),
        "reflection_calls": reflection.calls,
        "reflection_cost_usd": reflection.total_cost,
    }
    (run_dir / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(json.dumps(report, indent=1))
    log(f"best prompt: {run_dir / 'best_prompt.txt'}")


# -- entry point -------------------------------------------------------------


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(prog="prompt-tune", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    def config_arg(p):
        p.add_argument("--config", help="niinii.toml to read (default: the one niinii would load)")

    def run_args(p):
        config_arg(p)
        p.add_argument("--endpoint", help="OpenAI-compatible base URL without /v1 (default: niinii.toml)")
        p.add_argument("--model", help="model id (default: niinii.toml openai_model)")
        p.add_argument("--concurrency", type=int, default=8)
        p.add_argument("--prompt", help="system prompt file (default: chat.system_prompt from niinii.toml)")
        p.add_argument("--judge", action="store_true", help="also grade test outputs with Claude via Claude Code")
        p.add_argument("--judge-model", help="Claude Code --model for the judge")

    p = sub.add_parser("prepare")
    config_arg(p)
    p.add_argument("--train", type=int, default=60)
    p.add_argument("--val", type=int, default=60)
    p.add_argument("--test", type=int, default=120)
    p.add_argument("--test-vn-fraction", type=float, default=0.3)
    p.add_argument("--seed", type=int, default=0)
    p.set_defaults(fn=cmd_prepare)

    p = sub.add_parser("eval")
    run_args(p)
    p.add_argument("--split", default="val", choices=["train", "val", "test"])
    p.add_argument("--out")
    p.set_defaults(fn=cmd_eval)

    p = sub.add_parser("tune")
    run_args(p)
    p.add_argument("--name", help="run directory name under runs/")
    p.add_argument("--max-metric-calls", type=int, default=1500)
    p.add_argument("--minibatch", type=int, default=8)
    p.add_argument("--reflection-model", help="Claude Code --model (default: Claude Code's default)")
    p.add_argument("--reflection-effort", help="Claude Code --effort")
    p.add_argument("--max-reflection-cost", type=float, help="stop once reflection cost reaches this (USD)")
    p.add_argument("--seed", type=int, default=0)
    p.set_defaults(fn=cmd_tune)

    args = ap.parse_args(argv)
    args.fn(args)


if __name__ == "__main__":
    main()
