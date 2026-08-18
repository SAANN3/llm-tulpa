import axios from 'axios'

import { BACKEND_URL } from '../../config'

export interface InputExampleOut {
  text: string
}

/**
 * A short, randomly-picked placeholder string for the chat composer's empty input.
 * Served from the backend's background-refreshed cache — no request body needed.
 * Mirrors `POST /api/prompts/input_examples`. `signal` lets a caller actually cancel the
 * request (see `greet`'s docs on why this matters on a cache miss).
 */
export async function inputExample(signal?: AbortSignal): Promise<InputExampleOut> {
  const { data } = await axios.post<InputExampleOut>(`${BACKEND_URL}/api/prompts/input_examples`, undefined, { signal })

  return data
}
