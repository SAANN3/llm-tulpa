import {useState} from 'react'
import {httpsUrl} from '../api/tls/https-address.ts'
import {copyText} from '../utils/copy-text.ts'
import {Button, Label} from './primitives'
import {SettingsRow} from './settings-fields.tsx'

/** Hosts that mean "this machine": the page is secure there, but the address is no use to another device */
const LOCAL_HOSTS = ['localhost', '127.0.0.1', '::1', '[::1]']
/** How long the Copy button says it has copied */
const COPIED_MS = 1500

export interface SecurePageRowProps {
    /** The page can use the microphone (https, or localhost) */
    secure: boolean
    /** The port the frontend container serves https on, when it does */
    port: number | null
    /** This machine's addresses another device reaches it by (known to the owner), for a page opened on localhost */
    addresses: string[]
}

/** Whether this page can use the microphone and, if another device needs a different address for it, which: the secure
 * address on the host the page was opened by, or on this machine's addresses when it was opened on localhost */
export const SecurePageRow = ({secure, port, addresses}: SecurePageRowProps) => {
    const [copied, setCopied] = useState(false)
    const {hostname, protocol, pathname} = window.location
    const local = LOCAL_HOSTS.includes(hostname)
    const urls = port == null ? [] : (local ? addresses : [hostname]).map((host) => httpsUrl(host, port))
    const first = urls[0] ?? null

    const copy = async () => {
        if (first == null || !(await copyText(first))) return
        setCopied(true)
        setTimeout(() => setCopied(false), COPIED_MS)
    }
    const copyButton = first != null
        ? <Button variant="secondary" text={copied ? 'Copied' : 'Copy'} onClicked={() => void copy()}/>
        : null

    // A page that is https already is the secure address: nothing to say about another one
    if (secure && protocol === 'https:') {
        return <SettingsRow label="This page" help="The microphone can be used on this page."><Label className="voice-model__state" text="Secure"/></SettingsRow>
    }
    if (secure) {
        return (
            <SettingsRow label="This page"
                         help={`The microphone can be used on this page. ${first != null ? `From another device, open ${urls.join(' or ')}.` : port != null ? `From another device, open https://<this machine's address>:${port}.` : ''}`.trim()}>
                {copyButton ?? <Label className="voice-model__state" text="Secure"/>}
            </SettingsRow>
        )
    }
    return (
        <SettingsRow label="This page" accent
                     help={first != null ? `A browser only allows the microphone on a secure page, and this one is not. Open ${first} instead (the browser warns about the certificate once: continue anyway).`
                         : 'A browser only allows the microphone on a secure page (https, or localhost), and this one is neither. The frontend container serves a secure version when it is started with the start scripts.'}>
            {first != null ? <a className="link-button" data-variant="secondary" href={`${first}${pathname}`}>Open</a> : <Label className="voice-model__state" text="Not secure"/>}
            {copyButton}
        </SettingsRow>
    )
};
