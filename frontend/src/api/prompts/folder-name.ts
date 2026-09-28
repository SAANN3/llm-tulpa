import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { GreetOut } from './greet'

/** Generates a short folder name from a description of what it's for, or content to summarize */
export async function folderName(content: string): Promise<GreetOut> {
  const { data } = await axios.post<GreetOut>(`${BACKEND_URL}/api/prompts/folder_name`, { content })

  return data
}
