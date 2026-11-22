# Qoder and Qoder CN

Qoder is Alibaba's agentic coding product, shipped as two editions with
completely separate accounts, model pools and data directories. Tokens reports
both under the single client id `qoder`; the edition only namespaces the
deduplication keys.

| | International | China |
|---|---|---|
| CLI config root | `~/.qoder` | `~/.qoder-cn` |
| Config override | `QODER_CONFIG_DIR` | `QODERCN_CONFIG_DIR` |
| Transcripts | `<config>/projects/**/*.jsonl` | same |
| Segment logs | `<config>/logs/sessions/**/segments/*.jsonl` | same |
| IDE database | `Qoder/SharedClientCache/cache/db/local.db` | `QoderCN/…` |

The IDE directories resolve under the per-user app-data root: `%APPDATA%` on
Windows, `~/Library/Application Support` on macOS, `${XDG_CONFIG_HOME:-~/.config}`
on Linux. `QODER_HOME` / `QODER_CN_HOME`, when set, are consulted first and are
expected to contain `cache/db/local.db` (or the full
`SharedClientCache/cache/db/local.db`).

Both editions are discovered when `qoder` is enabled. `--client qoder` (and the
default scan) include both; a path that belongs to the China edition is never
deduplicated against an international one, because the two accounts are
different.

## Sources and precedence

1. **Transcripts** — Claude-Code-shaped JSONL. One assistant reply is written as
   several `type:"assistant"` lines that share `message.id`; only the last one
   carries `message.usage`. Fragments are assembled by `message.id`, then keyed
   by `message.usage.request_id`.
2. **Segment logs** — event streams; only `type:"model.response.completed"` is a
   per-request record and is keyed by `request_id`. The `turn.finished` roll-up
   carries the same counters again and is ignored. Segment logs are the only
   record of requests made with `--print --no-session-persistence`.
3. **IDE SQLite** — `chat_message` rows for `role='assistant'`. Only
   `id`, `session_id`, `request_id`, `token_info`, `model_info`, `gmt_create`,
   and the joined `chat_session.project_uri` / `project_name` /
   `preferred_model_info` are selected.
   Message bodies, tool output, titles and credentials are never read. The
   database is opened read-only and never checkpointed, cleaned or written.

Duplicates collapse on the request id across all three sources, so a request
present in a transcript **and** a segment log, replayed by a forked session, or
repeated in a restarted segment file is counted once. Scans are idempotent:
rescanning, warm cache or cold, produces the same total.

## Token accounting

Qoder's agent engine reports `input_tokens` as the whole prompt, with
`cache_read_input_tokens` a subset of it — OpenAI `prompt_tokens` semantics, not
Anthropic's net `input_tokens`. Real 1.1.76+ transcripts confirm this: the
recorded `context_usage_ratio` equals `input_tokens / context_window`
(28067 / 0.140335 = 200000, 23537 / 0.1307611 = 180000). So the parser reports:

| Tokens field | Source |
|---|---|
| `input` | `input_tokens − cache_read_input_tokens` |
| `cache_read` | `cache_read_input_tokens` |
| `cache_write` | `cache_creation_input_tokens` |
| `cache_write_1h` | `cache_creation.ephemeral_1h_input_tokens`, clamped to `cache_write` |
| `output` | `output_tokens` |
| `reasoning` | always 0 — Qoder's Anthropic-shaped block has no separate reasoning counter |

The IDE's `token_info` follows the same rule:
`{"prompt_tokens":39005,"cached_tokens":37485,"completion_tokens":67}` →
1520 uncached input, 37485 cache read, 67 output, 39072 total.

Counters are clamped at zero, cache reads are clamped to the prompt, and every
sum saturates, so a damaged row cannot overflow or inflate a day. A row that
records only `credits` / `context_usage_ratio`, omits `usage`, or zeroes every
token field is **not measured usage** and produces no message at all. Tokens does
not convert credits to tokens, does not estimate from reply length, and does not
invent historical usage.

## Model attribution

- `message.model` wins (the IDE's `model_info.model_key`, then the session's
  `preferred_model_info.model_key`, for SQLite rows).
- Qoder 1.1.82+ writes `"auto"` there and puts the real slot in the preceding
  `user.businessModelKey`; mid-session switches arrive as `runtime-config.model`.
- Routing slots (`auto`, `qmodel`, `qmodel_38max`, `qfmodel`, `dfmodel`,
  `gfmodel`, …) name a router or a moving business slot whose backing model
  changes over time. They are kept verbatim and attributed to the provider
  `qoder`, so they never match another client's `auto` price and never produce a
  guessed cost. Their rows are unpriced, not `$0.00`.
- BYOK models keep their vendor, including Qoder's own qualification:
  `deepseek/chat/deepseek-v3` → `deepseek-v3` + `deepseek`,
  `qoder/claude-sonnet-4-6` → `claude-sonnet-4-6` + `anthropic`,
  `qoder-cn/qwen3-coder-plus` → `qwen3-coder-plus` + `qwen`.

## Enabling real CLI tokens

Managed models bill in credits, and Qoder scrubs the token columns unless the
exposure flag is set in the environment the Qoder process inherits:

```sh
export QODER_EXPOSE_TOKEN_USAGE=1     # international
export QODERCN_EXPOSE_TOKEN_USAGE=1   # Qoder CN
```

New terminals (and restarted GUI apps) pick it up; already-running processes do
not. Only **subsequent** requests are recorded — nothing backfills history. This
is version-specific: it has been reported working on Qoder CN CLI 1.1.62 and
later 1.1.7x builds, and the desktop IDE ignores it (its SQLite store always has
real tokens). BYOK/custom models do not need it.

## Verified against a real installation

Checked read-only against Qoder CN desktop 0.4.x on Windows (2026-11-22):

- The scanner finds the real tree: 90 `logs/sessions/**/segments/*.jsonl` files
  and 7 `projects/**/*.jsonl` transcripts, and no
  `Qoder/SharedClientCache/cache/db/local.db` (the 0.4.x desktop no longer
  writes it).
- Every real assistant line carries **no** `message.usage` at all, and no
  `request_id`; the segment logs contain **no** `model.response.completed`
  events, and all 18 `turn.finished` roll-ups carry `input_tokens: 0`,
  `output_tokens: 0`, `cache_read_input_tokens: 0`,
  `cache_creation_input_tokens: 0`.
- `chat_session_context_usage` in the new desktop store states
  `tokenCountsAvailable: false`, `totalTokens: 0`, `apiUsage: null` — while
  still storing a `percentage`. So even Qoder's own context gauge has no token
  numerator there, and `turn_credits` (billing) is the only counter.
- Result: 97 discovered files, **0 measured messages**, and
  `tokens submit -c qoder --dry-run` reports "No usage data found to submit".
  That is the intended outcome for first-party models without token exposure:
  nothing is estimated, and no `$0.00` row is invented.

Why that installation had no usage at all: every session ran on a first-party
managed model (`message.model` is `auto`, slots `qmodel_latest_preview` /
`qmodel_latest` / `dfmodel`, and the BYOK tables are empty), which Qoder bills
in credits and, without token exposure, records without token counters. The
gate (`QODERCN_EXPOSE_TOKEN_USAGE=1` here) was verified unset in the shell, the
user environment and the shell profiles, and the desktop exposes no setting for
it. The three local sources then genuinely contain no token numbers, so the
honest total is zero — credits are not converted (no fixed credit-to-token
rate), and `percentage` is not multiplied by a guessed context window.

## Known limits

- **Newer desktop data layouts are not read.** Qoder desktop 0.4.x moved its app
  data into `com.qoder.app.stable` / `com.qodercn.app.stable`, where the stores
  are `main.sqlite`, `chat-session-turn-payload-buffer.sqlite`, and friends.
  Inspected read-only on the machine above: the per-message table
  (`chat_session_messages`) has `turn_credits` but no token column, and no
  token/usage key appears anywhere in its `payload_json`; `chat_session_context_usage`
  holds context-window snapshots. So per-request Token values do not exist there
  (matching issue #733's report), and Tokens deliberately does not read it. Only
  the documented `Qoder[CN]/SharedClientCache/cache/db/local.db` `chat_message`
  shape is used.
- **`turn.finished` is never counted.** It is a whole-turn roll-up; adding it to
  `model.response.completed` events would double count. If a future build fills
  the roll-up while emitting no per-request events, that usage stays uncounted —
  the parser logs the condition at debug level instead of guessing.
- **Managed-model history stays unknown.** Rows written before the exposure flag
  was enabled have no recoverable token count; they are omitted rather than
  estimated.
- **Transcript headers without a project** (a file directly under `projects/`)
  yield no workspace label; the session is still counted.
- **Non-persisted requests are counted from segment logs.** They are real Qoder
  model calls recorded by Qoder itself, keyed by their `request_id`. If Qoder is
  driven as a backend by another agent client, that client may also account for
  the same call under its own client id; Tokens does not try to guess the host.
- **`QODER_HOME` / `QODER_CN_HOME` path conventions** follow the community
  implementations (a `cache/db/local.db` root, with the full
  `SharedClientCache/...` fallback). A build that chooses another layout is not
  discovered automatically; use `scanner.extraScanPaths` if needed.

## References

Field layouts and semantics were cross-checked against the sources below, and
the outcome was verified on a real Qoder CN desktop installation (see above):

- Tokscale's Qoder parser and its follow-up fixes
  ([#1612](https://github.com/junhoyeo/tokscale/pull/1612),
  [#1621](https://github.com/junhoyeo/tokscale/pull/1621));
- Vibe Usage's `src/parsers/qoder.js` and `src/qoder-roots.js`
  (IDE SQLite columns, `chat_session` fallback, gross `prompt_tokens`);
- usageBar's `QoderCliSegmentSource` / `QwenWorkSegmentParser` (segment events,
  `request_id` deduplication, `turn.finished` exclusion) and
  `QoderIdeDetailScanner` / `QoderUsageEnvGate`;
- token-monitor's Qoder notes (credits vs tokens, CN field semantics).
