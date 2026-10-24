import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {StagedPlugin} from './types'

/** Has the backend fetch a git repository (a branch or tag, else the default branch) to be described; nothing of it runs */
export const stageExternalPluginGit = async (url: string, gitRef: string): Promise<StagedPlugin> => {
    const {data} = await axios.post<StagedPlugin>(`${BACKEND_URL}/api/plugins/external/stage/git`, {url, git_ref: gitRef || null})
    return data
};
