const TOKEN_KEY = 'auth_token'

export const getToken = (): string | null => {
    try {
        return localStorage.getItem(TOKEN_KEY)
    } catch {
        return null
    }
};

export const setToken = (token: string) => {
    try {
        localStorage.setItem(TOKEN_KEY, token)
    } catch {}
};

export const clearToken = () => {
    try {
        localStorage.removeItem(TOKEN_KEY)
    } catch {}
};
