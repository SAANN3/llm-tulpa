import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {AuthResponse} from './types'

/** Exchanges a username/password for a JWT and the signed-in user */
export const login = async (username: string, password: string): Promise<AuthResponse> => {
    const {data} = await axios.post<AuthResponse>(`${BACKEND_URL}/api/auth/login`, {username, password})
    return data
};
