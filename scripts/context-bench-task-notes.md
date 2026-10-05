Work only inside /tmp/tulpa-bench/work, a checkout of this project's repository. Do not change or create anything outside that folder.

Before you read anything, save a three-line plan for this task with chat.write_notes.

The task: explain, from the code itself, what happens in the backend (backend/src) between a request to POST /api/agent/chat and the request that goes out to the model, and between a tool result being stored and the next model request. Follow routes/agent, then Agent::chat, advance, advance_once, use_tool, continue_chat, ollama_history and the compaction path (maybe_compact, compact, pick_compaction_boundary, summarize) in facade/agent.rs. Read the real code rather than guessing, and when you have finished the routes, update your notes with what you found.

Write the result to /tmp/tulpa-bench/work/BENCH-OUT.md: per function its file and line, what it reads and writes in the database, and what can go wrong.
