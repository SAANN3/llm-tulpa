import {useEffect, useLayoutEffect, useRef, useState} from 'react'
import '../styles/model-log.scss'
import {getRuntimeLogs} from '../api/runtime/logs'
import {Frame} from '../components/frame.tsx'
import {Button, Div, Label} from '../components/primitives'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useGoBack} from '../hooks/use-go-back.ts'
import {useRuntime} from '../hooks/use-runtime.ts'
import {copyText} from '../utils/copy-text.ts'
import {errorReason} from '../utils/error-reason.ts'

const POLL_MS = 2000

/** The model server's own output, as a terminal across the page: where the layers went, how much memory it took,
 * why a load failed. Follows the end while it grows, unless the reader has scrolled up to read. */
const ModelLog = () => {
    useDocumentTitle('Model server log')
    const goBack = useGoBack('/models')
    const {status} = useRuntime()
    const [lines, setLines] = useState<string[]>([])
    const [error, setError] = useState<string | null>(null)
    const [copied, setCopied] = useState(false)
    const box = useRef<HTMLDivElement>(null)
    // Whether the reader is at the end of the log: new lines then scroll into view, but someone who
    // has scrolled up to read is left where they are
    const following = useRef(true)

    useLayoutEffect(() => {
        const el = box.current
        if (el && following.current) el.scrollTop = el.scrollHeight
    }, [lines])

    useEffect(() => {
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
    }, [])

    const copy = async () => {
        if (!(await copyText(lines.join('\n')))) return
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
    }

    return (
        <Div className="page center vbox model-log">
            <Frame className="model-log__panel" bodyClassName="model-log__body" title="Model server log"
                   actions={[{keys: ['c'], label: 'copy', run: () => void copy()}]} onEscape={goBack} escapeLabel="back">
                <Div className="model-log__head">
                    <Button variant="secondary" text="Back" onClicked={goBack}/>
                    <Label variant="secondary" className="model-log__meta"
                           text={[status?.model ?? 'nothing loaded', `${lines.length} lines`, 'live'].join(' · ')}/>
                    <Div className="model-log__spacer"/>
                    <Button variant="secondary" text={copied ? 'Copied' : 'Copy'} disabled={lines.length === 0} onClicked={() => void copy()}/>
                </Div>
                {error ? <Label variant="secondary" className="model-log__error" text={error}/> : null}
                <Div ref={box} className="model-log__lines"
                     onScroll={(e) => {
                         const el = e.currentTarget
                         following.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24
                     }}>
                    {lines.length === 0 ? <Label variant="secondary" text="Nothing logged yet."/> : null}
                    {lines.map((line, index) => <pre key={index} className="model-log__line">{line}</pre>)}
                </Div>
            </Frame>
        </Div>
    )
};

export default ModelLog
