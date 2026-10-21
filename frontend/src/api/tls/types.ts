/** What to show of a certificate */
export interface CertInfo {
    subject: string
    /** The names and addresses it is valid for */
    names: string[]
    /** RFC 3339 */
    not_after: string
    expired: boolean
}

export interface TlsStatus {
    /** This machine's addresses another device reaches it by: what to type after `https://` there */
    addresses: string[]
    /** A new certificate has been asked for and the container has not made it yet */
    regenerating: boolean
    /** `custom` (uploaded), `generated` (made by the frontend container) or `none` */
    active: 'custom' | 'generated' | 'none'
    custom: CertInfo | null
    generated: CertInfo | null
}
