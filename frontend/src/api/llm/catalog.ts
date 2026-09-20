import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Catalog} from './types'

/** Ollama's public model library, parsed by the backend (owner only) */
export const fetchCatalog = async (): Promise<Catalog> => {
    const {data} = await axios.get<Catalog>(`${BACKEND_URL}/api/llm/catalog`)
    return data
};
