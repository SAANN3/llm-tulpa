-- Sample data for a fresh `verify-env` instance, for the owner account the setup just created: a
-- model, a folder, three chats (one long agent chat with tool calls, failures, timings, slow calls
-- and a compaction summary), and a few background jobs — enough to fill every page of the UI,
-- including the stats tabs. The dates run back from now, so the data is always recent.
DO $$
DECLARE
    uid BIGINT := (SELECT id FROM users ORDER BY id LIMIT 1);
    model BIGINT;
    folder BIGINT;
    c1 BIGINT;
    c2 BIGINT;
    c3 BIGINT;
    call_message BIGINT;
    i INT;
    t TIMESTAMPTZ;
    slow BOOLEAN;
BEGIN
    INSERT INTO llm_models (provider_id, name)
        SELECT id, 'verify-model:latest' FROM llm_providers WHERE name = 'ollama'
        RETURNING id INTO model;
    UPDATE user_settings SET active_model_id = model WHERE user_id = uid;
    INSERT INTO folders (user_id, name) VALUES (uid, 'Sample folder') RETURNING id INTO folder;

    INSERT INTO chats (user_id, name, model_id, created_at, updated_at, folder_id)
        VALUES (uid, 'Sample: notes in a folder', model, now() - interval '9 days', now() - interval '8 days', folder)
        RETURNING id INTO c1;
    INSERT INTO chats (user_id, name, model_id, created_at, updated_at)
        VALUES (uid, 'Sample: long agent chat', model, now() - interval '9 days', now())
        RETURNING id INTO c2;
    INSERT INTO chats (user_id, name, model_id, created_at, updated_at)
        VALUES (uid, 'Sample: quick question', model, now() - interval '1 day', now() - interval '1 day')
        RETURNING id INTO c3;

    -- Two small chats of plain exchanges; the answers carry a code block
    FOR i IN 1..3 LOOP
        t := now() - interval '9 days' + i * interval '2 hours';
        INSERT INTO messages (chat_id, role, content, created_at) VALUES (c1, 'user', 'Sample question ' || i, t);
        INSERT INTO messages (chat_id, role, content, thinking, thought_duration_ms, created_at,
                              prompt_tokens, eval_tokens, eval_duration_ms, prompt_eval_duration_ms,
                              load_duration_ms, prompt_processed_tokens)
            VALUES (c1, 'assistant', 'Sample answer ' || i || E'\n\n```rust\nfn main() { println!("hi"); }\n```',
                    'Thinking it over.', 4000 + i * 300, t + interval '30 seconds',
                    1500 * i, 200 + i * 40, 3500, 600, 0, 1500 * i);
    END LOOP;
    INSERT INTO messages (chat_id, role, content, created_at) VALUES (c3, 'user', 'A quick question', now() - interval '1 day');
    INSERT INTO messages (chat_id, role, content, created_at, prompt_tokens, eval_tokens, eval_duration_ms,
                          prompt_eval_duration_ms, load_duration_ms, prompt_processed_tokens, thought_duration_ms)
        VALUES (c3, 'assistant', 'A quick answer.', now() - interval '1 day' + interval '9 seconds',
                800, 60, 900, 300, 0, 800, 2100);

    -- The long agent chat: 40 steps over about 8 days, each a user message, a tool call with its
    -- result (every ninth fails) and a reply. Every seventh reply is a slow call: a prompt the
    -- server had to evaluate from scratch.
    FOR i IN 1..40 LOOP
        t := now() - (40 - i) * interval '5 hours' - (random() * 4) * interval '1 hour';
        slow := i % 7 = 0;
        INSERT INTO messages (chat_id, role, content, created_at) VALUES (c2, 'user', 'Sample task step ' || i, t);

        INSERT INTO messages (chat_id, role, content, thinking, thought_duration_ms, created_at,
                              prompt_tokens, eval_tokens, eval_duration_ms, prompt_eval_duration_ms,
                              load_duration_ms, prompt_processed_tokens)
            VALUES (c2, 'assistant', '', 'I should read the file first.', 2500, t + interval '5 seconds',
                    20000 + i * 900, 80, 1200, 700, 0, 900)
            RETURNING id INTO call_message;
        INSERT INTO tool_calls (message_id, tool_name, arguments, position)
            VALUES (call_message, 'storage.read_file', '{"path": "/tmp/notes.txt"}', 0);
        INSERT INTO messages (chat_id, role, content, tool_name, tool_success, created_at)
            VALUES (c2, 'tool', CASE WHEN i % 9 = 0 THEN 'No such file' ELSE '{"content": "sample"}' END,
                    'storage.read_file', i % 9 <> 0, t + interval '6 seconds');

        INSERT INTO messages (chat_id, role, content, thinking, thought_duration_ms, created_at,
                              prompt_tokens, eval_tokens, eval_duration_ms, prompt_eval_duration_ms,
                              load_duration_ms, prompt_processed_tokens)
            VALUES (c2, 'assistant', 'Done with step ' || i || '.', 'All good.',
                    CASE WHEN slow THEN 95000 ELSE 3500 END, t + interval '10 seconds',
                    20000 + i * 900, 150 + i * 10, 2200,
                    CASE WHEN slow THEN 90000 ELSE 800 END, 0,
                    CASE WHEN slow THEN 20000 + i * 900 ELSE 1000 END);
    END LOOP;

    -- A compacted chat: a summary of its older history, and a context size to show
    UPDATE chats
        SET summary = 'Earlier in this chat: sample steps 1 to 20 were done, reading notes and fixing a bug.',
            last_prompt_tokens = 26000
        WHERE id = c2;

    INSERT INTO jobs (chat_id, command, status, exit_code, started_at, finished_at, notified, kind)
        VALUES (c2, 'sleep 5', 'exited', 0, now() - interval '3 days', now() - interval '3 days' + interval '5 seconds', true, 'process'),
               (c2, 'false', 'exited', 1, now() - interval '2 days', now() - interval '2 days' + interval '1 second', true, 'process'),
               (c2, 'sleep 1000', 'killed', NULL, now() - interval '1 day', now() - interval '1 day' + interval '30 seconds', true, 'process');
END
$$;
