import {createContext} from 'react'
import type {AuthResponse, User} from '../api/auth/types'

export interface AuthContextValue {
    user: User | null
    token: string | null
    loading: boolean
    login: (username: string, password: string) => Promise<void>
    logout: () => void
    setSession: (response: AuthResponse) => void
}

export const AuthContext = createContext<AuthContextValue | null>(null)
