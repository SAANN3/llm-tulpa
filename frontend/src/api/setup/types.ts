export interface SetupStatus {
    configured: boolean
    has_owner: boolean
}

export interface DatabaseForm {
    host: string
    port: number
    name: string
    user: string
    password: string
}
