import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { Settings } from './types'

/** The persisted user settings, or a 404 if none have been saved yet. Mirrors `GET /api/settings`. */
export async function getSettings(): Promise<Settings> {
  const { data } = await axios.get<Settings>(`${BACKEND_URL}/api/settings`)

  return data
}
