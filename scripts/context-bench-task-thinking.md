Work only inside /tmp/tulpa-bench/work, a checkout of this project's repository. Do not change or create anything outside that folder.

I want a design for making an agent turn keep running on the backend when the browser tab that started it is closed. Today the browser drives a turn: it calls POST /api/agent/chat, then loops on /api/agent/use_tool and /api/agent/continue. Read the code that matters (routes/agent, facade/agent.rs and its subagent_run module) before you answer.

Then think each of these scenarios through carefully and in order, working out step by step what state the system is in and what the design must do: (1) the tab is closed while a tool is running, (2) a permission prompt is pending and nobody answers, (3) two tabs are open on the same chat, (4) the model is switched in the middle of a turn, (5) the backend restarts during a turn, (6) a compaction fold starts in the middle of a turn. Check every conclusion against the code before you rely on it.

Write the design to /tmp/tulpa-bench/work/DESIGN-OUT.md: the mechanism, one section per scenario with the exact functions and lines involved, and a list of what could still go wrong.
