"""GEPA reflection model backed by Claude Code in headless mode (`claude -p`).

Runs with no tools, no MCP servers and no session persistence, from an
empty working directory so no project instructions are picked up. Uses the
account Claude Code is logged in with.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import threading
from pathlib import Path

SYSTEM_PROMPT = (
    "You are an expert prompt engineer. You improve system prompts for a model that "
    "translates Japanese visual novel lines into English. Answer with the requested "
    "artifact only."
)

# GEPA's default template asks for "niche and domain specific factual information"
# to be folded into the instruction; with script data that means character names and
# plot facts. The app supplies those per VN through the VNDB addendum, so the system
# prompt must stay VN-agnostic.
REFLECTION_TEMPLATE = """You are improving the system prompt of a Japanese-to-English visual novel translator. The translator is a small local model. Current system prompt:
```
<curr_param>
```

How the prompt is used:
- The system message is this prompt, followed (when available) by a VNDB-derived character list for the current VN.
- Earlier script lines arrive as the conversation that translated them: a user message "Translate into English:" plus the line, then the assistant's translation. The final user message is the one line to translate now.
- Dialogue lines look like `Name  \u300c...\u300d`; narration has no speaker. The reply is shown to the reader as-is.

Below are lines the translator handled with this prompt, its outputs, and feedback from an automatic grader (chrF++ of the translated text against a human reference translation, minus penalties for: echoed tags, added commentary, translating context lines instead of the final line, cut-off or incomplete output, a dialogue line losing its speaker, attributing a line to the wrong character, giving narration an invented speaker, and missing or misread names; any Japanese left in the output scores 0):
```
<side_info>
```

Write an improved system prompt.
- Fix the failure patterns that recur across several examples; don't chase one-off wording differences from the reference.
- Keep it general to any visual novel. Do not include character names, places, plot facts or terms specific to these examples; the per-VN character list provides those.
- Keep it concise and direct; a small model follows short, concrete rules better than long prose. Drop rules that don't change behavior.
- Keep the content-policy sentence about rewording and characters' ages.

Return only the new system prompt inside a single ``` block."""


class ClaudeCodeLM:
    """`LanguageModel` for GEPA. `total_cost` lets GEPA enforce
    `max_reflection_cost`; it is the cost Claude Code reports per call."""

    def __init__(
        self,
        model: str | None,
        effort: str | None,
        log_dir: Path,
        timeout_s: float = 900,
        system_prompt: str = SYSTEM_PROMPT,
        log_prefix: str = "reflection",
    ):
        exe = shutil.which("claude") or str(Path.home() / ".local" / "bin" / "claude")
        self.cmd = [
            exe, "-p",
            "--output-format", "json",
            "--tools", "",
            "--strict-mcp-config",
            "--no-session-persistence",
            "--system-prompt", system_prompt,
        ]
        self.log_prefix = log_prefix
        if model:
            self.cmd += ["--model", model]
        if effort:
            self.cmd += ["--effort", effort]
        self.timeout_s = timeout_s
        self.log_dir = log_dir
        self.total_cost = 0.0
        self.calls = 0
        self._cwd = tempfile.mkdtemp(prefix="prompt-tune-claude-")
        self._lock = threading.Lock()

    def __call__(self, prompt: str | list[dict]) -> str:
        if not isinstance(prompt, str):
            prompt = "\n\n".join(str(m.get("content", "")) for m in prompt)
        last_err = None
        for _ in range(3):
            proc = subprocess.run(
                self.cmd, input=prompt, cwd=self._cwd, capture_output=True,
                text=True, encoding="utf-8", timeout=self.timeout_s,
            )
            try:
                result = json.loads(proc.stdout)
            except json.JSONDecodeError:
                last_err = f"exit {proc.returncode}: {proc.stderr[-2000:] or proc.stdout[-2000:]}"
                continue
            if result.get("is_error"):
                last_err = result.get("result") or json.dumps(result)[:2000]
                continue
            with self._lock:
                self.total_cost += result.get("total_cost_usd") or 0.0
                self.calls += 1
                n = self.calls
            self.log_dir.mkdir(parents=True, exist_ok=True)
            (self.log_dir / f"{self.log_prefix}_{n:03d}.json").write_text(
                json.dumps({"prompt": prompt, "response": result["result"], "cost_usd": result.get("total_cost_usd")},
                           ensure_ascii=False, indent=1),
                encoding="utf-8",
            )
            return result["result"]
        raise RuntimeError(f"claude -p failed: {last_err}")
