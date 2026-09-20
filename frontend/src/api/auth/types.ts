export interface User {
    id: number
    username: string
    role: 'owner' | 'user'
    created_at: string
}

export interface AuthResponse {
    token: string
    user: User
}
