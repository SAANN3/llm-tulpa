import axios from 'axios'

import { BACKEND_URL } from '../../config'

/**
 * Resets user settings — a dev/testing hook for exercising the "settings not configured
 * yet" UI path. Mirrors `DELETE /api/settings`.
 */
export async function deleteSettings(): Promise<void> {
  await axios.delete(`${BACKEND_URL}/api/settings`)
}
