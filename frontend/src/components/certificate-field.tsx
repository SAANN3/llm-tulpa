import {useEffect, useState} from 'react'
import {regenerateCertificate} from '../api/tls/regenerate.ts'
import {removeCertificate} from '../api/tls/remove-certificate.ts'
import type {CertInfo, TlsStatus} from '../api/tls/types.ts'
import {errorReason} from '../utils/error-reason.ts'
import {CertificatePopup} from './popups/certificate-popup.tsx'
import {Button, Div, Label} from './primitives'
import {SettingsRow} from './settings-fields.tsx'

/** How often the status is read while a new certificate is being made */
const POLL_MS = 2000

export interface CertificateFieldProps {
    status: TlsStatus | null
    onChanged: () => void
}

const until = (cert: CertInfo): string => {
    const end = new Date(cert.not_after)
    return `${cert.expired ? 'expired' : 'valid until'} ${Number.isNaN(end.getTime()) ? cert.not_after : end.toLocaleDateString()}`
}

const covers = (cert: CertInfo, host: string): boolean => cert.names.some((name) => name.toLowerCase() === host.toLowerCase())

/** The HTTPS certificates of the frontend container (owner only): the one it made itself, which can be made again, and
 * putting in one's own */
export const CertificateField = ({status, onChanged}: CertificateFieldProps) => {
    const [open, setOpen] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const own = status?.custom ?? null
    const made = status?.generated ?? null
    const host = window.location.hostname
    const regenerating = status?.regenerating === true

    useEffect(() => {
        if (!regenerating) return
        const id = setInterval(onChanged, POLL_MS)
        return () => clearInterval(id)
    }, [regenerating, onChanged])

    const run = async (action: () => Promise<void>, fallback: string) => {
        setError(null)
        try {
            await action()
        } catch (e) {
            setError(errorReason(e, fallback))
        }
        onChanged()
    }

    const lifecycle = 'The server makes its own when the container starts, again when this machine\'s addresses are different at the next start, when it is missing and a month before it ends.'
    const notOnIt = made != null && !covers(made, host) && !regenerating
    const help = status == null ? 'Reading the certificates…'
        : regenerating ? 'Making a new one…'
            : own ? `In use: your own, ${own.names.length ? own.names.join(', ') : own.subject}, ${until(own)}.${made ? ` The server's own (for ${made.names.join(', ')}, ${until(made)}) waits for when you go back to it.` : ''}`
                : made ? `In use: the one the server made for itself, for ${made.names.join(', ')}, ${until(made)}. A browser warns about it once on each device: choose to continue. To avoid the warning, use a certificate your devices trust.${notOnIt ? ` This page's address (${host}) is not on it, so a browser will say it does not match: make a new one.` : ''} ${lifecycle}`
                    : 'The frontend container has not made a certificate yet. Start it (./start-native.sh or ./start-docker.sh) to get a secure page.'

    return (
        <>
            <SettingsRow label="Certificate" top stack accent={notOnIt && own == null} help={help}
                         below={error ? <Label className="voice-model__error" text={error}/> : null}>
                <Div className="voice-cert__buttons">
                    <Button variant="secondary" text={own ? 'Replace my certificate' : 'Use my own certificate'} onClicked={() => setOpen(true)}/>
                    {own ? <Button variant="secondary" text="Use the server's own" onClicked={() => void run(removeCertificate, 'The certificate could not be removed.')}/> : null}
                    <Button variant="secondary" text={regenerating ? 'Making…' : 'Make a new one'} disabled={regenerating}
                            onClicked={() => void run(regenerateCertificate, 'A new certificate could not be asked for.')}/>
                </Div>
            </SettingsRow>
            <CertificatePopup open={open} onClose={() => setOpen(false)} onStored={() => { setOpen(false); onChanged() }}/>
        </>
    )
};
