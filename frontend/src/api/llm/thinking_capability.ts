import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { ThinkingCapability } from '../agent/types'

/**
 * What the currently active model supports for `think` — fetched fresh every call,
 * not cached here or anywhere else client-side (mirrors the backend's own
 * no-persistence design, since the active model can change without either side
 * restarting). Meant to be called whenever a caller actually needs to know what to
 * offer (e.g. `UserInput` on mount), not once at app startup. Mirrors
 * `GET /api/llm/thinking_capability` on the backend.
 */
export async function getThinkingCapability(): Promise<ThinkingCapability> {
  const { data } = await axios.get<ThinkingCapability>(`${BACKEND_URL}/api/llm/thinking_capability`)
  return data
}
