import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Throws a staged plugin away without installing it */
export const discardStagedPlugin = async (id: string): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/plugins/external/stage/discard`, {id})
};
