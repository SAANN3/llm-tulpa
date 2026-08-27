import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { PluginInfo } from './types'

/** Every registered plugin, enabled and disabled alike. Mirrors `GET /api/plugins`. */
export async function getPlugins(): Promise<PluginInfo[]> {
  const { data } = await axios.get<PluginInfo[]>(`${BACKEND_URL}/api/plugins`)

  return data
}
