export interface SetupStatus {
    configured: boolean
    has_owner: boolean
    /** A newer release changed what the wizard sets up since the owner last completed it */
    update_available: boolean
}

export interface DatabaseForm {
    host: string
    port: number
    name: string
    user: string
    password: string
}
