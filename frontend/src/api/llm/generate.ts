import axios from 'axios'

import { BACKEND_URL } from '../../config'

export interface GenerateResponse {
  response: string
  model: string
  created_at: string
  thinking: string | null
}

/**
 * Raw one-shot completion, no history and no tools — the model responds to `prompt`
 * alone. `think` asks the model to reason before answering, and defaults to `true`.
 * Mirrors `POST /api/llm/generate` on the backend.
 */
export async function generate(prompt: string, think = true): Promise<GenerateResponse> {
  const { data } = await axios.post<GenerateResponse>(
    `${BACKEND_URL}/api/llm/generate`,
    { prompt, think },
  )

  return data
}
