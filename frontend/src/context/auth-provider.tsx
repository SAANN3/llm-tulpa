import {useEffect, useState, type ReactNode} from 'react'
import {AuthContext} from './auth-context.ts'
import {useSetup} from './use-setup.ts'
import {login as loginApi} from '../api/auth/login'
import {me as meApi} from '../api/auth/me'
import type {AuthResponse, User} from '../api/auth/types'
import {clearToken, getToken, setToken} from '../utils/auth-token'

export const AuthProvider = ({children}: { children: ReactNode }) => {
    const {status, loading: setupLoading} = useSetup()
    const [token, setTokenState] = useState<string | null>(getToken)
    const [user, setUser] = useState<User | null>(null)
    const [loading, setLoading] = useState(true)

    const ready = status?.configured === true && status.has_owner

    useEffect(() => {
        if (setupLoading) return

        if (!ready) {
            if (token) {
                clearToken()
                setTokenState(null)
            }
            setUser(null)
            setLoading(false)
            return
        }

        if (!token) {
            setLoading(false)
            return
        }

        let cancelled = false
        meApi()
            .then((u) => !cancelled && setUser(u))
            .catch(() => {
                if (cancelled) return
                clearToken()
                setTokenState(null)
                setUser(null)
            })
            .finally(() => !cancelled && setLoading(false))

        return () => {
            cancelled = true
        }
    }, [token, ready, setupLoading])

    const setSession = (response: AuthResponse) => {
        setToken(response.token)
        setTokenState(response.token)
        setUser(response.user)
        setLoading(false)
    }

    const login = async (username: string, password: string) => {
        setSession(await loginApi(username, password))
    }

    const logout = () => {
        clearToken()
        setTokenState(null)
        setUser(null)
    }

    return (
        <AuthContext.Provider value={{user, token, loading, login, logout, setSession}}>
            {children}
        </AuthContext.Provider>
    )
};
