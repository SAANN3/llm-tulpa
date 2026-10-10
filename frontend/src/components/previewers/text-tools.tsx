import {useState} from 'react'
import {Button} from '../primitives'
import {useWindowTools} from '../popups/base/window-tools.ts'
import {copyText} from '../../utils/copy-text.ts'

/** Wrap and Copy in the window's tool row for text shown whole (`w`, `c`), with what it is at the row's end; returns
 * whether long lines wrap. `wrapFirst` is how it starts: prose and logs read better wrapped, code doesn't */
export const useTextTools = (content: string | null, what: string, wrapFirst = false): boolean => {
    const [wrap, setWrap] = useState(wrapFirst)
    const [copied, setCopied] = useState(false)
    const copy = async () => {
        if (content == null || !(await copyText(content))) return
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1500)
    }
    const lines = content == null ? null : content.split('\n').length

    useWindowTools({
        node: (
            <>
                <Button variant="secondary" className={`preview-tool${wrap ? ' preview-tool--on' : ''}`} text="Wrap" title="Wrap long lines (w)"
                        onClicked={() => setWrap((now) => !now)}/>
                <Button variant="secondary" className="preview-tool" text={copied ? 'Copied' : 'Copy'} title="Copy all (c)"
                        onClicked={() => void copy()}/>
            </>
        ),
        meta: lines == null ? undefined : `${what}${what ? ' · ' : ''}${lines} ${lines === 1 ? 'line' : 'lines'}`,
        scalable: true,
        actions: [
            {keys: ['w'], label: 'wrap', run: () => setWrap((now) => !now)},
            {keys: ['c'], label: 'copy', run: () => void copy()},
        ],
    }, [wrap, copied, content, what])

    return wrap
}
