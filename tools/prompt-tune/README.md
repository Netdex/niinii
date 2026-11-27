# prompt-tune

Tunes the chat translator's system prompt with [GEPA](https://github.com/gepa-ai/gepa)
against [VNTL](https://huggingface.co/datasets/lmg-anon/VNTL-v3.1-1k), using the
local model niinii translates with. The reflection model is Claude Code in
headless mode (`claude -p`), so it runs on your Claude Code login.

## How it measures what the app does

- **Prompts:** `prompt_tune/render.py` builds the same messages as the chat
  backend: the system prompt plus VNDB character list, then each earlier line as
  the user/assistant turn that translated it, then the line to translate. Every
  user line starts with `Translate into English:`. It is a port of `build_prompt` in
  `niinii/src/translator/chat.rs` and of the VNDB addendum in
  `niinii/src/vndb.rs`; keep them in sync when either changes.
- **Request settings:** temperature, `max_tokens`, reasoning effort and similar
  come from `[chat]` in `niinii.toml`.
- **VNDB context:** VNTL rows carry no VN id. `vn_map.json` maps each
  character's Japanese name to the VNDB ids of the VNs it appears in, and a
  row belongs to the VN most of its characters map to. The VNDB character list
  for that VN goes into the prompt exactly as the app builds it.

## Data split

Cases are drawn from both VNTL splits (13 VNs; the val split alone is a single
VN). Consecutive rows are chained into continuous scripts, and each case is a
line with the context the app would hold there: everything before it, trimmed
by `max_context_tokens` from `niinii.toml` the way the chat backend trims.
The VNTL reference translations stand in for the model's earlier replies, so
the eval sees ideal history and cannot show mistakes locking in.

- **train:** lines the reflection model reads.
- **val:** GEPA's selection set.
- **test:** drawn from VNs that appear in neither train nor val, so the
  test score shows whether a prompt carries over to unseen VNs. Test is only
  scored after tuning.

## Score

chrF++ of the translated text against the reference, minus penalties. The
speaker prefix is stripped from both sides before chrF, so formatting doesn't
earn credit. The speaker is checked separately, using the reference's speaker
tag: on dialogue the English speaker name must appear in any form (given name,
full name, alias) with any separator; narration must not gain a speaker. Any
Japanese left in the output scores 0.

| Penalty | For |
| --- | --- |
| 0.3 | Translating earlier lines instead of the final line |
| 0.3 | Line attributed to a different character |
| 0.3 | Narration turned into dialogue with an invented speaker |
| 0.2 | Dialogue that lost its speaker |
| 0.2 | Echoed prompt text (`Translate into English:`) or markup |
| 0.2 | Notes, explanations or other commentary |
| 0.15 | Cut-off or incomplete output |
| 0.1 | Missing or misread names in the line itself |

Each penalty is also written as feedback for the reflection model.

chrF against a single reference punishes valid paraphrases and misses meaning
errors, so differences under about 0.03 on the default test set are noise.

## LLM judge

`--judge` also grades outputs with Claude through Claude Code: meaning and
fluency (1-5), whether the speaker is right, and a list of errors. Lines are
judged in batches of 20, blind to which prompt produced them, and cached in
`data/judgments.sqlite` by (case, output). In `tune` it grades the final test
scoring only; GEPA itself uses the metric above.

## Usage

```powershell
cd tools/prompt-tune
uv run prompt-tune prepare
uv run prompt-tune eval --endpoint https://host --model gemma-4-26b-a4b-qat --split test --judge
uv run prompt-tune tune --endpoint https://host --model gemma-4-26b-a4b-qat --name gemma --judge
```

- **Endpoint:** the base URL without `/v1`.
- **Prompt:** `chat.system_prompt` from `niinii.toml`, or `--prompt FILE`.
- **Response cache:** responses are cached in `data/responses.sqlite`, keyed by
  the exact request, so re-running a candidate is free. Delete it if the
  server's model changes under the same name.

Each run writes `runs/<name>/`:

- `best_prompt.txt`
- `candidates.json`: every prompt GEPA tried, with its val score.
- `report.json`: val and test scores, seed vs best.
- `test-*.jsonl`: per-case test outputs.
- `reflections/`: every Claude Code call.
- `gepa/`: GEPA's state. Rerun with the same `--name` to resume.

To apply a result, paste `best_prompt.txt` into the system prompt row of the
translator window, or into `chat.system_prompt` in `niinii.toml`.
