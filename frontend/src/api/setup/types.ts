export interface SetupStatus {
    configured: boolean
    has_owner: boolean
    /** A newer release changed what the wizard sets up since the owner last completed it */
    update_available: boolean
    /** The setup revision the owner last completed (0 when none): an update asks only about what a later revision added */
    setup_revision: number
}

export interface DatabaseForm {
    host: string
    port: number
    name: string
    user: string
    password: string
}
