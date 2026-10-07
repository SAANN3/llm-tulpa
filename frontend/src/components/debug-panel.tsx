import {useState} from 'react'
import '../styles/debug-panel.scss'
import {Button, Div, Label} from './primitives'
import {BACKEND_URL} from '../config'
import {useDebug, useDebugLines} from '../hooks/use-debug.ts'
import {copyText} from '../utils/copy-text.ts'

const COPIED_SHOWN_MS = 1500

/** The newest debug lines on screen, for a device with no console (a phone); nothing while the debug switch is off */
export const DebugPanel = () => {
    const debug = useDebug()
    const lines = useDebugLines()
    const [open, setOpen] = useState(true)
    const [copied, setCopied] = useState<boolean | null>(null)

    if (!debug) return null

    // Oldest first, with what a reader of the pasted text would otherwise have to ask for
    const onCopy = async () => {
        const text = [`page ${window.location.origin}`, `backend ${BACKEND_URL}`, navigator.userAgent, ...lines].join('\n')
        setCopied(await copyText(text))
        setTimeout(() => setCopied(null), COPIED_SHOWN_MS)
    }

    return (
        <Div className="debug-panel">
            <Div className="debug-panel__head">
                <Label text={`debug (${lines.length})`}/>
                <Div className="debug-panel__buttons">
                    <Button className="debug-panel__button" variant="secondary" text={copied == null ? 'Copy' : copied ? 'Copied' : 'Failed'} onClicked={onCopy}/>
                    <Button className="debug-panel__button" variant="secondary" text={open ? '-' : '+'} onClicked={() => setOpen((prev) => !prev)}/>
                </Div>
            </Div>
            {open ? (
                <Div className="debug-panel__lines">
                    {[...lines].reverse().map((line, index) => <Div key={lines.length - index}>{line}</Div>)}
                </Div>
            ) : null}
        </Div>
    )
};
