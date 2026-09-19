import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {AuthResponse} from '../auth/types'

/** Creates the first (owner) account — only valid while no users exist — and signs in */
export const createOwner = async (username: string, password: string): Promise<AuthResponse> => {
    const {data} = await axios.post<AuthResponse>(`${BACKEND_URL}/api/setup/owner`, {username, password})
    return data
};
