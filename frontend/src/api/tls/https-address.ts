/**
 * The port the frontend container serves https on, when it does: the container writes it to `/tls.json` beside the page.
 * Asked of the page's own origin, not the backend, so it works while the backend is down; `null` where there is no such
 * file (the dev server, a page served some other way).
 */
export const getHttpsPort = async (): Promise<number | null> => {
    try {
        const response = await fetch('/tls.json', {cache: 'no-store'})
        const {port} = (await response.json()) as { port?: number }
        return port ?? null
    } catch {
        // Not a container's page: there is no https to point to
        return null
    }
};

/** `https://host:port`, with an IPv6 address in brackets */
export const httpsUrl = (host: string, port: number): string => `https://${host.includes(':') ? `[${host}]` : host}:${port}`

/** Where the secure version of this page is, on the host the page was loaded from; `null` when there is none */
export const getHttpsAddress = async (): Promise<string | null> => {
    const port = await getHttpsPort()
    return port == null ? null : httpsUrl(window.location.hostname, port)
};
