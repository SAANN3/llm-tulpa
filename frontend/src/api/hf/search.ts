import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {HfRepo} from './types'

/** The orders a search can come in, biggest or newest first */
export type HfSort = 'downloads' | 'likes' | 'createdAt' | 'lastModified'

export interface HfSearchPage {
    repos: HfRepo[]
    /** Where the next page starts; null on the last one */
    next_cursor: string | null
    has_token: boolean
}

/** A page of Hugging Face repositories with GGUF files, in `sort` order; `cursor` is the page before's `next_cursor` */
export const searchHf = async (q: string, sort: HfSort = 'downloads', cursor?: string): Promise<HfSearchPage> => {
    const {data} = await axios.get<HfSearchPage>(`${BACKEND_URL}/api/hf/search`, {params: {q, sort, ...(cursor ? {cursor} : {})}})
    return data
};
