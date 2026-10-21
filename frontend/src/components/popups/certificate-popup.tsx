import {useRef, useState} from 'react'
import {setCertificate} from '../../api/tls/set-certificate.ts'
import {errorReason} from '../../utils/error-reason.ts'
import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface CertificatePopupProps {
    open: boolean
    onClose: () => void
    onStored: () => void
}

/** A file the user picked: its name and what it holds */
interface Picked {
    name: string
    text: string
}

/** Puts in the owner's own HTTPS certificate and private key, as the PEM files they have */
export const CertificatePopup = ({open, onClose, onStored}: CertificatePopupProps) => {
    const [certificate, setCertificateFile] = useState<Picked | null>(null)
    const [key, setKeyFile] = useState<Picked | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)
    const certInput = useRef<HTMLInputElement>(null)
    const keyInput = useRef<HTMLInputElement>(null)

    const pick = (set: (file: Picked) => void) => async (files: FileList | null) => {
        const file = files?.[0]
        if (file) set({name: file.name, text: await file.text()})
    }
    const close = () => {
        setCertificateFile(null)
        setKeyFile(null)
        setError(null)
        onClose()
    }
    const store = async () => {
        if (!certificate || !key) return
        setBusy(true)
        setError(null)
        try {
            await setCertificate(certificate.text, key.text)
            setCertificateFile(null)
            setKeyFile(null)
            onStored()
        } catch (e) {
            setError(errorReason(e, 'The certificate could not be stored.'))
        } finally {
            setBusy(false)
        }
    }

    return (
        <Popup open={open} onClose={close} title="Your own certificate" width={520} actions={[]}>
            <Label text="Pick the certificate and its private key, as PEM files (the key without a password). The server checks that they belong together before it uses them."/>
            <input ref={certInput} type="file" accept=".pem,.crt,.cer" style={{display: 'none'}}
                   onChange={(e) => { void pick(setCertificateFile)(e.target.files); e.target.value = '' }}/>
            <input ref={keyInput} type="file" accept=".pem,.key" style={{display: 'none'}}
                   onChange={(e) => { void pick(setKeyFile)(e.target.files); e.target.value = '' }}/>
            <Div className="voice-cert__pick">
                <Button variant="secondary" text="Certificate…" onClicked={() => certInput.current?.click()}/>
                <Label variant="secondary" text={certificate?.name ?? 'No file chosen'}/>
            </Div>
            <Div className="voice-cert__pick">
                <Button variant="secondary" text="Private key…" onClicked={() => keyInput.current?.click()}/>
                <Label variant="secondary" text={key?.name ?? 'No file chosen'}/>
            </Div>
            {error ? <Label className="voice-model__error" text={error}/> : null}
            <PopupActions confirmLabel={busy ? 'Checking…' : 'Use them'} onConfirm={() => void store()} confirmDisabled={!certificate || !key || busy}
                          onCancel={close} emphasis="confirm"/>
        </Popup>
    )
};
