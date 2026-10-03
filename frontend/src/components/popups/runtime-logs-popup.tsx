import {useEffect, useState} from 'react'
import {getRuntimeLogs} from '../../api/runtime/logs'
import {errorReason} from '../../utils/error-reason.ts'
import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'

export interface RuntimeLogsPopupProps {
    open: boolean
    onClose: () => void
}

const POLL_MS = 2000

/** The model server's own output: where the layers went, how much memory it took, why it failed */
export const RuntimeLogsPopup = ({open, onClose}: RuntimeLogsPopupProps) => {
    const [lines, setLines] = useState<string[]>([])
    const [error, setError] = useState<string | null>(null)

    useEffect(() => {
        if (!open) return
        let cancelled = false
        const load = async () => {
            try {
                const logs = await getRuntimeLogs()
                if (!cancelled) {
                    setLines(logs.lines)
                    setError(null)
                }
            } catch (e) {
                if (!cancelled) setError(errorReason(e, 'Could not read the log.'))
            }
        }
        void load()
        const id = setInterval(() => void load(), POLL_MS)
        return () => {
            cancelled = true
            clearInterval(id)
        }
    }, [open])

    return (
        <Popup open={open} onClose={onClose} title="Model server log" width={760}>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            <Div className="models__log">
                {lines.length === 0 ? <Label variant="secondary" text="Nothing logged yet."/> : null}
                {lines.map((line, index) => <pre key={index} className="models__log-line">{line}</pre>)}
            </Div>
            <Div className="popup__actions">
                <Button variant="secondary" text="Close" onClicked={onClose}/>
            </Div>
        </Popup>
    )
};
