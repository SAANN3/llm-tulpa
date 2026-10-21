import {useNavigate} from 'react-router-dom'
import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'

export type VoiceHint = { kind: 'insecure' } | { kind: 'no-model' } | { kind: 'error'; message: string }

export interface VoiceHintPopupProps {
    /** Why voice input can't be used, or what went wrong; null while the popup is closed */
    hint: VoiceHint | null
    isOwner: boolean
    /** Where the secure (https) version of this page is, when the frontend container offers one */
    httpsAddress: string | null
    onClose: () => void
}

/** Says why the microphone button can't be used and what to do (open the secure page, install a voice model), or what
 * went wrong when it was used */
export const VoiceHintPopup = ({hint, isOwner, httpsAddress, onClose}: VoiceHintPopupProps) => {
    const navigate = useNavigate()
    const openSettings = () => {
        onClose()
        navigate('/settings/voice')
    }
    const secureUrl = httpsAddress ? `${httpsAddress}${window.location.pathname}` : null

    return (
        <Popup open={hint != null} onClose={onClose} title="Voice input" actions={[]}>
            {hint?.kind === 'insecure' ? (
                <>
                    <Label text="Your browser only lets a page use the microphone when it is a secure one (https, or localhost)."/>
                    {secureUrl
                        ? <Label variant="secondary" text="This server has a secure version. Its certificate is signed by the server itself, so the browser warns about it once: choose to continue anyway."/>
                        : <Label variant="secondary" text="This page is not served over https. Settings → Voice says how to set it up."/>}
                    <Div className="popup__actions">
                        <Button variant="secondary" text="Close" onClicked={onClose}/>
                        {secureUrl ? <a className="link-button" data-variant="primary" href={secureUrl}>Open the secure page</a> : null}
                    </Div>
                </>
            ) : hint?.kind === 'error' ? (
                <>
                    <Label text="Voice input stopped."/>
                    <Label variant="secondary" text={hint.message}/>
                    <Div className="popup__actions">
                        <Button text="OK" onClicked={onClose}/>
                    </Div>
                </>
            ) : (
                <>
                    <Label text="No voice model is installed."/>
                    <Label variant="secondary" text={isOwner ? 'Install one in Settings → Voice. It is a download of a few hundred megabytes.' : 'The owner of this server can install one in Settings → Voice.'}/>
                    <Div className="popup__actions">
                        <Button variant="secondary" text="Close" onClicked={onClose}/>
                        {isOwner ? <Button text="Open voice settings" onClicked={openSettings}/> : null}
                    </Div>
                </>
            )}
        </Popup>
    )
};
