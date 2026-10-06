# Agent components and a per-chat turn runner: design note

Status: the backend is built (phases 1-7 below). The frontend still calls the endpoints this note drops; switching it is phase 8.

## Words used here
- **Turn:** everything between a user message and the model's final answer. It is a loop: the model replies, maybe asks for tools, the tools run, the model is asked again, until it answers without asking for a tool.
- **Step:** one pass of that loop (one model call and what follows it).
- **Run:** one execution of a turn from start to its end: a final answer, an error, a stop, or "waiting for permission".
- **TurnRunner:** the part that loops the steps of a run. Today the browser is the runner (and `drive_subagent` is a second one, for sub-agents).

## Why
- `facade/agent.rs` is one 2,700-line object with ~40 methods and 16 fields. Its domains (turn, prompt assembly, compaction, tools and permissions, sub-agents) all see all of its state.
- The browser runs the loop (`use-messages.ts`, `chat.tsx`: `chat` → `use_tool` → `continue`, repeat), and the backend only does one step per request. A closed or killed tab stops a turn (chat 263), and two tabs on one chat can both run it. A second copy of the loop lives in the backend for sub-agents (`subagent_run.rs`).
- The public surface is small (`chat`, `continue_chat`, `regenerate`, `flush_notices`, `can_use_tool`, `use_tool`, `allow_scope`, plus construction and binding the sub-agent runner), so the inside can change shape freely.

Goal: split the agent into private components with one owner each for state, write the turn as explicit steps, and have one backend `TurnRunner` replace both loops. Non-goals: changing the prompt bytes the model sees (the cache depends on them), changing tools, stores or schema.

## Components
All live in `facade/agent/`, private to it. `Agent` stays as the thin public facade and wires them.

| Component | Owns (state) | Does | Uses |
|---|---|---|---|
| `Prompts` | nothing | every fixed prompt text, as consts or small functions | none |
| `History` | nothing | builds what the model is sent from a chat's stored rows: system message (rules, summary, key facts, pinned messages, notes), stubs for cleared results, thinking caps | Prompts |
| `ModelCall` | `turn_holds` | picks provider and launch profile, sizes the reply cap, holds the model server for a turn, makes one request with the reply policy (unusable replies, regeneration, cut-off continuation), and can abort it | providers, launch |
| `Compaction` | `compaction_backoff` | the trigger, clearing plan, the notes request, the fold (summary and facts), back-off | History, ModelCall, one-shot helper |
| `ToolCalls` (`tool_calls.rs`) | `running`, `live_subagents` | permission and scope storage, run one tool, pending calls, interrupted calls | tool service, permission store |
| `Notices` | nothing | turns finished jobs and sub-agent results into notices | job store |
| `RunTracker` (`run_tracker.rs`) | one run's start time, current call start, tokens, stop signal | what the runner and the steps share about a run | none |
| `Turn` | nothing | one step: build request, call, persist, hand back tool calls, run maintenance | History, ModelCall, Compaction, Tools, Notices |
| `TurnRunner` | the registry of active runs | loops steps until the run ends, with a permission policy; see below | Turn, Tools |
| `Subagents` (`subagent_run.rs`, still `impl Agent`) | the slot | starts a sub-agent as a background job; its loop is a `TurnRunner` run with the sub-agent policy | TurnRunner (through the existing `Weak` runner trait) |

```
Prompts ──► History ──► Compaction ──┐
               │            ▲        ▼
               └──► ModelCall ──────► Turn ──► TurnRunner ──► Agent (public API)
         Tools ───────────────────────┘             ▲
         Notices ─────────────────────┘             │
                                       Subagents ───┘ (Weak, trait)
```
Dependencies point one way. A component never holds `Agent`. The only cycle (sub-agents start turns, turns can start sub-agents) is cut with the trait and `Weak` that already exist.

One more piece sits a level up, because `PromptFacade` needs it too: `facade/one_shot.rs`, a helper for "system instruction, untrusted data as its own user message, thinking off, no tools, validate the reply, one retry with a sharper closing, fallback". The summary and the facts use it, and so do `chat_name` and `folder_name`. (`greet` and the input examples use the plain `generate` call and stay as they are. The notes request is not a one-shot: it extends the live prompt, so it belongs to `Compaction` and `ModelCall`.)

## One step of a turn
1. `Turn` loads the chat and settings, asks `Tools` for the tool set, `History` for the messages, `ModelCall` for the provider and cap.
2. It adds the per-request notes to the last message (date, read-only streak). They are the one thing that changes the tail, never the front.
3. `ModelCall` makes the request and classifies the reply (usable, tool call as text, empty, cut off in thinking); policy decides retry, auto-continue, or give up.
4. `Turn` persists the reply and its tool calls, and publishes progress.
5. `Compaction` runs when the measured prompt crossed the trigger: notes request first (while the prompt is still cached), then clearing, then a fold only if clearing was not enough.
6. The step returns the reply and its pending tool calls. It does not loop; the `TurnRunner` does.

Invariants the split must keep, each with a test: the front of the prompt changes only at a compaction; the notes request uses the chat's own thinking setting; stubs and pinned blocks are byte-stable between folds; a clearing pass never runs without the notes request before it.

## The turn runner
**Today:** two loops (browser, sub-agent) with different permission handling and no shared guard.

**Target:** one runner per chat, kept in a registry. Starting a run on a chat that already has one is refused (409), which closes the two-tab gap.

**A run:** start from a prompt (or from an answer to a permission request); loop `Turn` steps; for each pending tool call check permission, run the tool, continue; end on a final reply, an error, a stop, or a step limit.

**Permission never waits.** When a pending tool call needs permission the user has not given, the run ends in the state *waiting for permission*. Nothing stays alive. The state is read from stored data: the last message is an assistant message with a tool call that has no result. The user's answer starts a new run that continues from there. `auto_confirm` (per user) grants without stopping. Sub-agents use the unattended policy: grant what the escalation offers, or refuse. The model-server hold that expires after 10 minutes stays as it is.

**Stop** ends the run at once: the in-flight model call is aborted, its partial reply is discarded, and the chat is left as of its last stored message. A tool that is already running finishes and its result is stored.

**Run state and events.** `GET /turn?chat_id=` returns `idle`, `running` or `waiting_for_permission`; for a running chat also when the run started, when the current model call started, and the tokens spent so far in the run (counted per finished call, since model calls are not streamed). A page that is reloaded, or a chat that is switched to, reads this once and then follows the event stream: it can show "thinking for 1m 30s, N tokens" from the real start time with a plain client-side timer, instead of restarting from zero. Every open tab follows the same events, so tabs stay in step, and `POST /turn` on a chat with an active run is refused (409). The events: reply stored, tool started and finished, progress, run ended (with why: answered, failed, stopped, step limit, waiting for permission).

**Jobs wake the chat.** A finished background job or sub-agent starts a run on its own, with no browser open (the `JobFinished` event exists). Hours later the backend alone picks the turn up again and runs until the model answers, a stop, a step limit, or a tool that needs permission. A chat that is *waiting for permission* is not woken: the notice is queued and goes to the model with the next step after the user answers. Waking it would drop the pending tool call, because a new message supersedes unresolved calls.

**Step limit.** A per-user setting (next to `auto_confirm`), empty by default, meaning unlimited. When it is set and reached, the last step is told: "you are about to be stopped until the user continues; write your conclusion and where you left off; you cannot run tools in this reply, text only"; any tool call in that reply is refused and its text is kept. The run then ends with the reason "step limit". The user continues by sending a message as usual.

**After a restart:** the existing interrupted-turn cleanup marks what was cut short. No automatic resume.

**Messaging plugins** keep calling "run to completion" (a run they wait on), as sub-agents do today.

### Endpoints
| Today (`/api/agent/…`) | Becomes | Why |
|---|---|---|
| `POST /chat` | `POST /turn`: returns 202 at once, `409` if the chat has an active run | the runner works in the background |
| `POST /continue` | dropped | continuing after tool results is the runner's own loop; the browser was its only caller |
| `POST /use_tool` | dropped | the runner runs the tools; same reason |
| `POST /can_use_tool` | dropped | `GET /turn?chat_id=` returns the same pending call, why it needs permission, and the run state, in one place |
| `POST /allow_scope` | `POST /answer` with `decisions: [{index, allowance: permanent \| only_now \| deny}]`, index into `pending` of `GET /turn`; also resumes the run | one call replaces `allow_scope` + `use_tool` + `continue` |
| `POST /job_notices` | dropped | the server starts the reply when a job finishes (the `JobFinished` event exists) |
| `POST /regenerate` | kept, returns 202 | it runs through the same runner and events |
| (new) `POST /stop` | stops the active run of a chat | the stop button |
| (new) `GET /turn?chat_id=` | run state | page reload, and the permission prompt |

## Code style
- One struct per component holding only the handles it needs (cloned `Arc`s) and the state it owns, built in dependency order in `Agent::new`. No `impl Agent` blocks outside the facade once the split is done.
- Pure logic is written as associated functions without `self` on the component's struct (`impl Notes { fn reply(..) }`), private unless another component calls them, then `pub(super)`. Tests sit in the same file. Loading rows and building from them are separate, so the prompt build is a function of plain inputs.
- Prompt text only in `prompts.rs`. No inline prompt strings in components.
- Components return `ErrorService` and use `?`; a best-effort step swallows its error with a comment saying why.
- Keep the comments that explain why when code moves; new comments say why, not what.
- Tests: pure units; store-backed tests against a throwaway Docker Postgres, as before; a byte-identity test for the prompt build; a live smoke on a small window after each phase.

## Order of work
Backend first, the frontend after it, and the frontend is not touched while the backend is built. Each backend phase compiles, passes the tests, and ends with a live smoke before the next starts.
1. `Prompts` and `one_shot`.
2. `History`.
3. `ModelCall`, including abort.
4. `Compaction`, with clearing, notes and pinned folded in.
5. `Tools` and `Notices`.
6. `Turn` as steps; `Agent` becomes the facade.
7. `TurnRunner`, events, the new endpoints (the old ones removed); sub-agents move onto it. The current UI cannot run turns from here until phase 8, so nothing is released in between.
8. Frontend, in one switch: the loop and its hooks removed, the page driven by the events, a stop button, the permission prompt from `GET /turn` and `POST /answer`.

## As built
- **Where things are:** `agent/runner.rs` (`TurnRunner`: the registry of runs, the loop, the permission policy, the job wake-up, regenerate, answers), `agent/turn.rs` (one step), `agent/run_tracker.rs`. `Agent` keeps the public methods (`start_turn`, `start_regenerate`, `answer`, `stop`, `turn_state`, `reply`, `bind_job_waker`) and delegates.
- **Policy:** `Attended` (a user's chat: a call needing permission ends the run, unless the user has auto-confirm) and `Subagent` (grant with auto-confirm, refuse without; the run ends at `llm.return_agent`; 100 model calls at most).
- **Events** (`ServerEvent`): `run_started`, `messages_changed` (read what is newer than the last message you have), `tool_started`, `turn_progress` (as before), `run_ended` with `answered`, `failed` (with `detail`), `stopped`, `step_limit` or `waiting_for_permission`, plus when the run started and the tokens it generated. `turn_progress` also carries the step. The last end is also kept in memory and returned by `GET /turn` as `last_end`, so a page opened after a failure can say why nothing is going on.
- **`GET /turn` on a chat with no run** records a cut-short allowed tool call as interrupted (it calls `ToolCalls::can_use`); with a run going it does not touch the chat.
- **Step limit:** `user_settings.max_turn_steps` (schema 14; 0 in `POST /settings` removes it, null means none). On the last allowed step the newest message of that request gets `prompts::step_limit_note`; a tool call in that reply is dropped from the stored message and the text stays; the run ends with `step_limit`. The wording matters: the first wording was ignored in 6 of 6 live runs (the model wrote "Second call to os.get_date." and tried the call again, which was refused), the current one was obeyed in 6 of 6 (on a task that asked for more calls than the limit allowed). Results over 25% of the window are stored cut (`RESULT_WINDOW_FRACTION` in `tool_calls.rs`), with a line saying how to ask for the rest; and after a run ends, `TurnRunner` looks once more for jobs that finished during its last model call.
- **Stop** drops the model call in flight (`tokio::select!` in `Turn::ask`), so the request is closed and nothing of the reply is stored; a tool already running finishes, and tools queued behind it stay pending.
- **Messaging plugins** use `Agent::reply`: one step, no run and no registry entry.
- **Regenerate** checks (`check_regenerable`) after claiming the chat, so no other run can change the newest message between the check and the step.

## Found while building
- A tool result that makes the *next* request exceed the window failed with a 502 and left the chat stuck: the compaction check at the start of a continue used the prompt size measured *before* the result arrived, so nothing triggered, and every retry failed the same way. Seen at a 16k window with a 100-entry `chat.list_messages` result. `Turn::make_room` now estimates the next request (last measured size plus the characters added since the model's last reply, at 3 characters a token) and compacts first; in the live check it fired at an estimate of 15,672 tokens and folded.
- A fresh tool result bigger than the window could not be folded away (it is the newest message, and the fold keeps the tail), so the request was refused with a 400 by the server. Seen at a 16k window with `chat.get_messages` for four ids. `ToolCalls` now stores any result past a quarter of the window cut, with a line saying so; a 40,163-character command output on a 16k window was stored as 12,288 characters plus that line, and the turn went on.

## Decided in review
- The last step at the step limit keeps the same tool list, so the model server's cached prompt still serves it, and any tool call in that reply is refused while its text is kept.
- A chat waiting for permission is not woken by a finished job; the notice is queued and goes to the model with the next step after the answer.

## Open decisions
None.
