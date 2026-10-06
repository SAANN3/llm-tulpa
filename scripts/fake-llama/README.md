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
3. Start the fake: `python3 scripts/fake-llama/fake_llama.py 18300 40960` (port, window).

## Run
    python3 scripts/fake-llama/run_scenarios.py PATH/TO/settings.json PROFILE_ID [BACKEND_URL] [FAKE_URL] [scenario ...]

It signs its own token with the settings file's JWT secret. `TRACE=1` prints a failure's traceback. Each scenario makes its own chat.

## Adding a scenario
Write a function in `run_scenarios.py` with `@scenario`: `script(...)` the replies, `turn(chat, prompt)` (or start one and poll),
then check `messages(chat)`, `state(chat)` and `calls()` (what the fake was sent: kind `main`, `summary`, `facts` or `notes`, whether it carried
tools, the messages). A reply item is documented in the header of `fake_llama.py`; `match` ties a reply to a request by its system message, so a
parent chat and its sub-agent can share one script.
