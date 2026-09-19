import {useEffect, useState, type ReactNode} from 'react'
import axios from 'axios'
import {AuthContext} from './auth-context.ts'
import {useSetup} from './use-setup.ts'
import {setClientHandler} from '../api/client.ts'
import {login as loginApi} from '../api/auth/login'
import {me as meApi} from '../api/auth/me'
import type {AuthResponse, User} from '../api/auth/types'
import {clearToken, getToken, setToken} from '../utils/auth-token'

/** How long to wait before re-asking who the token belongs to after a non-401 failure */
const RETRY_DELAY_MS = 3000

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
        let retry: ReturnType<typeof setTimeout> | undefined

        const load = () => {
            meApi()
                .then((u) => {
                    if (cancelled) return
                    setUser(u)
                    setLoading(false)
                })
                .catch((e) => {
                    if (cancelled) return
                    // Only a 401 means the token is bad (the interceptor has already cleared it and
                    // signalled `onUnauthorized`). Anything else — a network blip, a restarting
                    // backend — must not log the user out, so keep the token and try again.
                    if (axios.isAxiosError(e) && e.response?.status === 401) {
                        setLoading(false)
                        return
                    }
                    retry = setTimeout(load, RETRY_DELAY_MS)
                })
        }
        load()

        return () => {
            cancelled = true
            clearTimeout(retry)
        }
    }, [token, ready, setupLoading])

    // The session died server-side (401 on any request): drop it here so the route guard
    // redirects to /login without a page reload.
    useEffect(() => setClientHandler('onUnauthorized', () => {
        setTokenState(null)
        setUser(null)
    }), [])

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
