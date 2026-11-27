"""GEPA adapter: one component (the chat system prompt), evaluated by
translating each case with the local model and scoring it."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from gepa.core.adapter import EvaluationBatch

from .dataset import Case
from .metric import Score, score
from .render import build_messages
from .task import Completion, Translator


COMPONENT = "system_prompt"


@dataclass
class Rollout:
    case: Case
    output: str
    result: Score
    # judge.Judgment, when the run was judged.
    judgment: Any = None


def evaluate_prompt(
    cases: list[Case], addenda: dict[str, str], translator: Translator, system_prompt: str
) -> list[Rollout]:
    prompts = [build_messages(system_prompt, addenda.get(c.vn_id), c.context, c.line) for c in cases]
    completions: list[Completion] = translator.translate(prompts)
    return [
        Rollout(c, comp.text, score(c, comp.text, comp.finish_reason, comp.error))
        for c, comp in zip(cases, completions)
    ]


class NiiniiAdapter:
    # GEPA checks this attribute; None selects its reflection-based proposer.
    propose_new_texts = None

    def __init__(self, addenda: dict[str, str], translator: Translator):
        self.addenda = addenda
        self.translator = translator

    def evaluate(self, batch: list[Case], candidate: dict[str, str], capture_traces: bool = False):
        rollouts = evaluate_prompt(batch, self.addenda, self.translator, candidate[COMPONENT])
        return EvaluationBatch(
            outputs=[r.output for r in rollouts],
            scores=[r.result.score for r in rollouts],
            trajectories=rollouts if capture_traces else None,
        )

    def make_reflective_dataset(self, candidate, eval_batch, components_to_update):
        records = []
        # Worst first: the reflection model should see the failures.
        for r in sorted(eval_batch.trajectories, key=lambda r: r.result.score):
            context = [f"{ja}\n  -> {en}" for ja, en in r.case.context[-3:]]
            records.append(
                {
                    "Inputs": {
                        "preceding lines and their translations (last 3)": "\n".join(context) or "(none)",
                        "line to translate": r.case.line,
                    },
                    "Generated Outputs": r.output or "(empty)",
                    "Feedback": r.result.feedback() + f"\nReference translation: {r.case.reference}",
                }
            )
        return {name: records for name in components_to_update}
