import axios from 'axios'

import { BACKEND_URL } from '../../config'

export interface GreetOut {
  response: string
  model: string
  created_at: string
  thinking: string | null
}

/**
 * A short, lively greeting for the "no chat open yet" landing page. Served from the
 * backend's background-refreshed cache — no request body needed, the backend already
 * knows the persisted timezone/display name. Mirrors `POST /api/prompts/greet`. `signal`
 * lets a caller actually cancel the request (not just ignore its result) — useful since
 * this can take many seconds on a cache miss, and a caller that's since navigated away
 * shouldn't leave it tying up a browser connection slot in the background.
 */
export async function greet(signal?: AbortSignal): Promise<GreetOut> {
  const { data } = await axios.post<GreetOut>(`${BACKEND_URL}/api/prompts/greet`, undefined, { signal })

  return data
}
