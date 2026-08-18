import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { Settings } from './types'

/** Persists user settings. Mirrors `POST /api/settings`. */
export async function setSettings(settings: Settings): Promise<void> {
  await axios.post(`${BACKEND_URL}/api/settings`, settings)
}
