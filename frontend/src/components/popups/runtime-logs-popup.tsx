import {useEffect, useLayoutEffect, useRef, useState} from 'react'
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
    const box = useRef<HTMLDivElement>(null)
    // Whether the reader is at the end of the log: new lines then scroll into view, but someone who
    // has scrolled up to read is left where they are
    const following = useRef(true)

    useLayoutEffect(() => {
        const el = box.current
        if (el && following.current) el.scrollTop = el.scrollHeight
    }, [lines])

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
        <Popup open={open} onClose={onClose} title="Model server log" width={760} actions={[]}>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            <Div ref={box} className="models__log"
                 onScroll={(e) => {
                     const el = e.currentTarget
                     following.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24
                 }}>
                {lines.length === 0 ? <Label variant="secondary" text="Nothing logged yet."/> : null}
                {lines.map((line, index) => <pre key={index} className="models__log-line">{line}</pre>)}
            </Div>
            <Div className="popup__actions">
                <Button variant="primary" text="Close" onClicked={onClose}/>
            </Div>
        </Popup>
    )
};
