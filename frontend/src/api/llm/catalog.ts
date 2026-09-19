import axios from 'axios'
import {BACKEND_URL} from '../../config'

interface CatalogResponse {
    html: string | null
}

/** The raw ollama.com/library page (proxied), or null when it couldn't be reached */
export const fetchCatalog = async (): Promise<string | null> => {
    const {data} = await axios.get<CatalogResponse>(`${BACKEND_URL}/api/llm/catalog`)
    return data.html
};
