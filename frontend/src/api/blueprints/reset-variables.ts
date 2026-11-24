import axios from 'axios'
import {BACKEND_URL} from '../../config'

export const resetBlueprintVariables = async (id: number, revision: number, nodeId: string | undefined, signal: AbortSignal) => {
    await axios.delete(`${BACKEND_URL}/api/blueprints/${id}/variables`, {
        params: {expected_revision: revision, node_id: nodeId},
        signal,
    })
}
