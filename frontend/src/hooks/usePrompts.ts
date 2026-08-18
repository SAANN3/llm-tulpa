import { useCallback } from 'react'

import { chatName as chatNameApi } from '../api/prompts/chat_name'
import { greet as greetApi } from '../api/prompts/greet'
import { inputExample as inputExampleApi } from '../api/prompts/input_examples'

/** Prompt-facade calls — one-shot generations against a predefined prompt, no chat history. */
export function usePrompts() {
  // Stable reference (no dependencies) so callers can safely list it in a `useEffect`
  // dependency array without that effect re-firing on every unrelated render.
  const greet = useCallback(async (signal?: AbortSignal): Promise<string> => {
    const result = await greetApi(signal)
    return result.response
  }, [])

  const inputExample = useCallback(async (signal?: AbortSignal): Promise<string> => {
    const result = await inputExampleApi(signal)
    return result.text
  }, [])

  const chatName = useCallback(async (content: string): Promise<string> => {
    const result = await chatNameApi(content)
    return result.response
  }, [])

  return { greet, inputExample, chatName }
}
