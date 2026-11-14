# Scripted model server for the turn logic

`fake_llama.py` answers like `llama-server` (`/v1/chat/completions`, `/props`, `/slots`, `/metrics`, `/health`) with replies a test
writes in advance, and `run_scenarios.py` drives a real backend against it through the public API and checks what was stored and
what the model was sent. A scenario takes a fraction of a second where the same check against a real model takes minutes and
depends on the model doing what is asked.

What it is for: the runner and the turn (stop, permission waits and answers, the step limit, the retry and cut-off paths, folds and
what the next request carries, the result cap, job and sub-agent wake-ups, regenerate, the tools switch, the events, two chats
at once). What it cannot say: anything about a real chat template or model quality, the GPU, or a backend that is killed.

## Setup
1. A backend whose settings have `llama_cpp.external_url` pointing at the fake (it then neither starts nor stops a server), on its own port
   and its own `files_dir`/`jobs_dir`, and a Postgres it can use (`scripts/verify-env` sets one up; or any throwaway one).
2. A registered llama.cpp model with a launch profile in that database (the chat is bound to it; the fake answers whatever the model). The
   profile's context window should be 40,960 for the fold scenarios, whose numbers assume it.
3. Start the fake: `python3 scripts/fake-llama/fake_llama.py 18300 40960` (port, window). With `FAKE_LEVELS=xhigh,medium,low` in its environment its template offers those thinking levels, so a page's thinking menu lists them.

## Run
    python3 scripts/fake-llama/run_scenarios.py PATH/TO/settings.json PROFILE_ID [BACKEND_URL] [FAKE_URL] [scenario ...]

It signs its own token with the settings file's JWT secret. `TRACE=1` prints a failure's traceback. Each scenario makes its own chat.

A memory request (the question a fold asks about the user) is answered with `{"memories":[]}` unless `POST /__memory` queued a text for it.

A streamed request that asks for `return_progress` gets the prompt's progress before its first token (four reports, the reply item's `cached` tokens counted as read from the start), and one that asks for `timings_per_token` gets the speed so far with every chunk, as `llama-server` sends them. `prompt_delay` and `piece_delay` space them out; `POST /__slow` sets them for the compaction's own requests, so a page can be looked at while a fold goes on.

## Adding a scenario
Write a function in `run_scenarios.py` with `@scenario`: `script(...)` the replies, `turn(chat, prompt)` (or start one and poll),
then check `messages(chat)`, `state(chat)` and `calls()` (what the fake was sent: kind `main`, `memory`, `summary`, `facts` or `notes`, whether it carried
tools, the messages). A reply item is documented in the header of `fake_llama.py`; `match` ties a reply to a request by its system message, so a
parent chat and its sub-agent can share one script.


`blueprint_tools_use_shared_permissions_without_model_calls` also drives explicit Tool nodes:
default approval, manual scopes/denial, stop during a permission wait, service-chat visibility and
output bindings. It asserts that these nodes make no model calls. It uses only temporary files.

`blueprint_generate_binds_text_and_cancels_without_a_chat` checks a single tool-free completion,
text output bindings, model/sampling/effort parameters, provider errors and output limits,
unexpected tool calls, cancellation and queue continuation without creating a chat.

`blueprint_agent_uses_the_turn_runner_reuses_chat_and_obeys_permissions` drives Agent nodes
through ordinary tool rounds, fresh/reused hidden chats, model switching, manual decisions,
stopping waits/generation and mandatory compaction with notes/memory writes disabled.

`stop_closes_a_permission_wait_without_running_the_tool` checks ordinary chat Stop during a
permission pause, pending-call cleanup and continuing that chat without replaying the cancelled write.

`blueprint_schedules_use_the_queue_and_record_readiness_failures` checks interval ticks, disabling,
manual starts, immutable workflow content and failed readiness without model calls.

`blueprint_plugin_nodes_use_both_sdks_and_existing_graph_bindings` uses the two hello examples
to check exact versions, input/output references, enabling/disabling and tool-free graph execution.

`blueprint_variables_are_run_local_and_operations_use_typed_values` checks ordered assignments,
live variable reads versus stored output snapshots, fresh defaults on each run, static errors,
formatting and all arithmetic/boolean/list operations without model requests.
