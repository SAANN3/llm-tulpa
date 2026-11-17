import {useCallback, useRef, useState} from 'react'
import {flushSync} from 'react-dom'
import {Frame} from '../frame'
import {LazyList, type LazyListHandle} from '../lazy-list'
import {Button, Div, Label} from '../primitives'
import {useBlueprintHistory} from '../../hooks/blueprint-editor/use-blueprint-history'
import {progressText, statusText} from '../../utils/blueprint-runs'

const when = (value: string | null) => value ? new Date(value).toLocaleString() : ''

interface HistoryPanelProps {
    blueprintId: number
    revision: number
}

export const HistoryPanel = ({blueprintId, revision}: HistoryPanelProps) => {
    const [selected, setSelected] = useState<number | null>(null)
    const list = useRef<LazyListHandle>(null)
    const firstRow = useRef<HTMLDivElement>(null)
    const updateList = useCallback((update: () => void) => {
        if (list.current && firstRow.current) {
            // Appending older runs should leave the reader in place, rather than follow the bottom.
            list.current.preserveViewportPosition(firstRow.current, () => flushSync(update))
        } else {
            list.current?.jumpToTop()
            flushSync(update)
        }
    }, [])
    const history = useBlueprintHistory(blueprintId, selected, updateList)
    const detail = history.detail?.run.id === selected ? history.detail : null
    const back = () => setSelected(null)
    return (
        <Div className="blueprint-editor__runs blueprint-editor__history">
            {history.error ? <Div className="blueprint-editor__error"><Label text={history.error}/></Div> : null}
            <Frame title="History" actions={[]} active={false} bodyClassName="blueprint-editor__history-body"
                   className={selected === null ? undefined : 'blueprint-editor__history-hidden'}>
                <LazyList ref={list} onBottomReached={() => { void history.loadMore() }} className="blueprint-editor__history-list">
                    {history.loading ? <Label text="Loading runs…"/> : history.items.length === 0 ? (
                        <Label variant="secondary" text="No runs yet."/>
                    ) : history.items.map((run, index) => (
                        <Div key={run.id} ref={index === 0 ? firstRow : undefined}>
                            <Button variant="secondary"
                                    className="blueprint-editor__run-history" onClicked={() => setSelected(run.id)}>
                                {when(run.created_at)} · revision {run.revision} · {run.scheduled_for ? `Scheduled ${when(run.scheduled_for)}` : 'Manual'} · {statusText(run.status)}
                            </Button>
                        </Div>
                    ))}
                </LazyList>
            </Frame>
            {selected !== null ? (
                <Frame key={selected} title={`Run ${selected}`} actions={[]} active={false}
                       bodyClassName="blueprint-editor__history-body">
                    <Div className="blueprint-editor__run-actions">
                        <Button variant="secondary" text="Back" onClicked={back}/>
                    </Div>
                    <Div className="blueprint-editor__history-detail">
                        {history.detailError ? <Label text={history.detailError}/> : null}
                        {detail ? (
                            <>
                                <Label text={detail.run.revision === revision
                                    ? `Snapshot of revision ${detail.run.revision}, the revision open in the editor.`
                                    : `Snapshot of revision ${detail.run.revision}. The open graph is revision ${revision}; this list is the snapshot that ran.`}/>
                                <Label variant="secondary" text={`${detail.run.scheduled_for ? 'Scheduled' : 'Manual'} · ${statusText(detail.run.status)} · ${progressText(detail.run.progress)}`}/>
                                {detail.run.error ? <Label text={detail.run.error}/> : null}
                                {detail.workflow.nodes.map(node => {
                                    const state = detail.state.nodes[node.id]
                                    return (
                                        <Div key={node.id} className="blueprint-editor__run-row">
                                            <Label text={`${node.name} · ${statusText(state?.status ?? 'pending')}`}/>
                                            {state?.error ? <Label variant="secondary" text={state.error}/> : null}
                                            {state && Object.keys(state.outputs).length > 0 ? (
                                                <Label variant="secondary" className="blueprint-editor__run-output" text={JSON.stringify(state.outputs).slice(0, 240)}/>
                                            ) : null}
                                        </Div>
                                    )
                                })}
                            </>
                        ) : !history.detailError ? <Label text="Loading run…"/> : null}
                    </Div>
                </Frame>
            ) : null}
        </Div>
    )
}
