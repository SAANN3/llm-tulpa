import {useEffect, useState} from 'react'
import {Navigate, useNavigate, useParams} from 'react-router-dom'
import {Layout} from 'pixelarticons/react'
import type {Schedule} from '../api/blueprints/types'
import {Frame} from '../components/frame'
import {Button, Div, Label} from '../components/primitives'
import {ConfirmPopup} from '../components/popups/base/confirm-popup'
import {ConnectionInspector} from '../components/blueprint-editor/connection-inspector'
import {UnsavedChangesPopup} from '../components/popups/unsaved-changes-popup'
import {keyBelongsToFocus, useKeyLayer} from '../hooks/use-key-layer'
import {useDocumentTitle} from '../hooks/use-document-title'
import {useBlueprintDocument} from '../hooks/blueprint-editor/use-blueprint-document'
import {EditorMenubar} from '../components/blueprint-editor/editor-menubar'
import {NodeCatalog} from '../components/blueprint-editor/node-catalog'
import {GraphCanvas} from '../components/blueprint-editor/graph-canvas'
import {NodeInspector} from '../components/blueprint-editor/node-inspector'
import {DocumentSettings} from '../components/blueprint-editor/document-settings'
import {HistoryPanel} from '../components/blueprint-editor/history-panel'
import {RunPermission, RunsPanel, RunStartPopup} from '../components/blueprint-editor/run-panel'
import {liveMarks, progressText, statusText} from '../utils/blueprint-runs'
import {useBlueprintRuns} from '../hooks/blueprint-editor/use-blueprint-runs'
import {nodeReferences} from '../utils/blueprint-graph'
import {useBlueprintGraph} from '../hooks/blueprint-editor/use-blueprint-graph'
import {useEditorResources} from '../hooks/blueprint-editor/use-editor-resources'
import {EDITOR_VIEWS} from '../utils/blueprint-views'
import '../styles/blueprint-editor.scss'

const BlueprintEditor = () => {
    const {id, view = 'graph'} = useParams()
    const navigate = useNavigate()
    const editor = useBlueprintDocument(Number(id))
    const {document, catalog, dirty, busy, loading, error, conflict, report} = editor
    useDocumentTitle(document?.blueprint.name ?? 'Blueprint')
    const [query, setQuery] = useState('')
    const [pane, setPane] = useState<'catalog' | 'inspector' | null>(null)
    const [paletteVisible, setPaletteVisible] = useState(true)
    const [inspectorVisible, setInspectorVisible] = useState(true)
    const [confirm, setConfirm] = useState<'back' | 'reload' | null>(null)
    const [runConfirm, setRunConfirm] = useState(false)
    const runs = useBlueprintRuns(Number(id))
    const graph = useBlueprintGraph({
        document, locked: !!busy, edit: editor.edit,
        onInspect: () => { setPane('inspector'); setInspectorVisible(true) },
    })
    const {
        selected, selectedEdge, node, edge, dataLink, dataReference, viewport, graphMessage, removing,
        select, selectEdge, selectData, addNode, connectControl, bindInput,
        removeEdge, removeData, requestDelete, deleteSelection, reorderEdge, moveNode, renameNode, editKind,
    } = graph
    const resources = useEditorResources()
    const path = (next: string) => `/blueprints/${id}/${next}`

    const back = () => {
        if (window.history.state?.idx > 0) navigate(-1)
        else navigate('/blueprints')
    }
    const requestBack = () => {
        if (busy) return
        if (dirty) setConfirm('back')
        else back()
    }
    const requestReload = () => {
        if (dirty) setConfirm('reload')
        else editor.reload()
    }
    const changeView = (next: string) => {
        navigate(path(next), {replace: true})
        setPane(null)
    }
    const renameDocument = (name: string) => {
        editor.edit(value => ({...value, blueprint: {...value.blueprint, name}}))
    }
    const changeSchedule = (schedule: Schedule) => {
        editor.edit(value => ({...value, blueprint: {...value.blueprint, schedule}}))
    }
    const discard = () => {
        setConfirm(null)
        if (confirm === 'back') back()
        else editor.reload()
    }
    const saveAndContinue = async () => {
        const action = confirm
        const saved = await editor.save()
        setConfirm(null)
        if (!saved) return
        if (action === 'back') back()
        else editor.reload()
    }
    const requestRun = () => {
        if (!document || busy || runs.acting) return
        if (dirty) setRunConfirm(true)
        else void runs.start(document.blueprint.revision)
    }
    const saveAndRun = async () => {
        const saved = await editor.save()
        setRunConfirm(false)
        if (saved) await runs.start(saved.revision)
    }

    useEffect(() => {
        if (!dirty) return
        const unload = (event: BeforeUnloadEvent) => {
            event.preventDefault()
            event.returnValue = ''
        }
        window.addEventListener('beforeunload', unload)
        return () => window.removeEventListener('beforeunload', unload)
    }, [dirty])

    const handleKey = (event: KeyboardEvent) => {
        if ((event.ctrlKey || event.metaKey) && !event.altKey) {
            // Physical keys keep editor shortcuts working in non-Latin layouts too.
            const key = event.code || `Key${event.key.toUpperCase()}`
            if (key === 'KeyS') {
                if (dirty) void editor.save()
                return true
            }
            if (key === 'KeyZ' && !keyBelongsToFocus(event)) {
                if (event.shiftKey) editor.redo()
                else editor.undo()
                return true
            }
            return false
        }
        if (keyBelongsToFocus(event)) return false
        if (event.key === 'Escape') {
            select(null)
            setPane(null)
            return true
        }
        if (event.key === 'Delete' && view === 'graph') { requestDelete(); return true }
        const viewIndex = Number(event.key) - 1
        if (event.key >= '1' && event.key <= '9' && EDITOR_VIEWS[viewIndex]) {
            navigate(path(EDITOR_VIEWS[viewIndex].id), {replace: true})
            return true
        }
        return false
    }
    useKeyLayer(true, handleKey)

    const menus = {
        file: [
            {label: 'Save', shortcut: 'Ctrl S', disabled: !dirty || !!busy || conflict, onSelect: () => { void editor.save() }},
            {label: 'Reload', disabled: !!busy, onSelect: requestReload},
        ],
        edit: [
            {label: 'Undo', shortcut: 'Ctrl Z', disabled: !editor.canUndo || !!busy, onSelect: editor.undo},
            {label: 'Redo', shortcut: 'Ctrl Shift Z', disabled: !editor.canRedo || !!busy, onSelect: editor.redo},
            {label: 'Delete selection', shortcut: 'Delete', disabled: !!busy || (!edge && !dataLink && (!node || node.kind.type === 'entry')), onSelect: requestDelete},
        ],
        view: [
            {label: 'Node catalog', toggled: paletteVisible, onSelect: () => setPaletteVisible(value => !value)},
            {label: 'Node inspector', toggled: inspectorVisible, onSelect: () => setInspectorVisible(value => !value)},
            {label: 'Fit graph', onSelect: () => viewport.current?.fit()},
        ],
    }
    const workspaceClass = [
        'blueprint-editor__workspace',
        !paletteVisible ? 'catalog-hidden' : null,
        !inspectorVisible ? 'inspector-hidden' : null,
        pane ? `mobile-${pane}` : null,
    ].filter(Boolean).join(' ')

    if (!EDITOR_VIEWS.some(item => item.id === view)) return <Navigate to={path('graph')} replace/>

    return (
        <Div className="page blueprint-editor">
            <Frame title="~/blueprints" className="blueprint-editor__frame"
                   bodyClassName="blueprint-editor__body" actions={[]} active={false}>
                <EditorMenubar view={view} busy={!!busy} menus={menus}
                               onView={changeView} onBack={requestBack} onKeyDown={handleKey}/>
                <Div className="blueprint-editor__document">
                    <Div className="blueprint-editor__name">
                        <Layout width={22} height={22}/>
                        <strong>{document?.blueprint.name ?? 'Blueprint'}</strong>
                        <span>{dirty ? 'Modified' : document ? `Revision ${document.blueprint.revision}` : ''}</span>
                    </Div>
                    <Div className="blueprint-editor__actions">
                        <Button variant="secondary" disabled={!document || !!busy}
                                onClicked={() => { void editor.validate() }}>
                            {busy === 'validating' ? 'Validating…' : 'Validate'}
                        </Button>
                        <Button disabled={!dirty || !!busy || conflict} onClicked={() => { void editor.save() }}>
                            {busy === 'saving' ? 'Saving…' : 'Save'}
                        </Button>
                        <Button variant="secondary" disabled={!document || !!busy || runs.acting || conflict} onClicked={requestRun}>
                            Run
                        </Button>
                    </Div>
                </Div>
                {resources.toolsError ? <Div className="blueprint-editor__error"><Label text={resources.toolsError}/></Div> : null}
                {error ? (
                    <Div className="blueprint-editor__error">
                        <Label text={error}/>
                        <Button variant="secondary" disabled={!!busy} onClicked={requestReload}>Reload</Button>
                    </Div>
                ) : null}
                {runs.error ? <Div className="blueprint-editor__error"><Label text={runs.error}/></Div> : null}
                {graphMessage ? (
                    <Div className="blueprint-editor__error">
                        <Label text={graphMessage}/>
                        <Button variant="secondary" text="Dismiss" onClicked={graph.clearMessage}/>
                    </Div>
                ) : null}
                {loading ? (
                    <Div className="blueprint-editor__loading">Loading blueprint…</Div>
                ) : document && catalog ? (
                    <>
                        {view === 'graph' ? (
                            <>
                                <Div className="blueprint-editor__mobile-tools">
                                    <Button variant="secondary" onClicked={() => setPane(pane === 'catalog' ? null : 'catalog')}>
                                        Nodes
                                    </Button>
                                    <span>{document.workflow.nodes.length} nodes</span>
                                    <Button variant="secondary" onClicked={() => setPane(pane === 'inspector' ? null : 'inspector')}>
                                        Inspector
                                    </Button>
                                </Div>
                                <Div className={workspaceClass}>
                                    <NodeCatalog catalog={catalog} query={query} onQuery={setQuery} onClose={() => setPane(null)}
                                                 onAdd={addNode} onDrag={() => setPane(null)} locked={!!busy}
                                                 hasEntry={document.workflow.nodes.some(item => item.kind.type === 'entry')}/>
                                    <GraphCanvas key={id} workflow={document.workflow} catalog={catalog}
                                                 selected={selected} onSelect={select} locked={!!busy}
                                                 viewportRef={viewport} onMove={moveNode} selectedEdge={selectedEdge}
                                                 onSelectEdge={selectEdge} onControl={connectControl} onData={bindInput} onRemoveEdge={removeEdge}
                                                 selectedData={dataLink} onSelectData={selectData} onRemoveData={removeData}
                                                 marks={liveMarks(runs.live, document.blueprint.revision)}/>
                                    {edge ? (
                                        <ConnectionInspector type="control" edge={edge} workflow={document.workflow} locked={!!busy}
                                                             onDelete={requestDelete} onOrder={reorderEdge} onClose={() => setPane(null)}/>
                                    ) : dataLink && dataReference ? (
                                        <ConnectionInspector type="data" link={dataLink} reference={dataReference}
                                                             workflow={document.workflow} locked={!!busy}
                                                             onDelete={requestDelete} onClose={() => setPane(null)}/>
                                    ) : (
                                        <NodeInspector node={node} catalog={catalog} locked={!!busy} workflow={document.workflow}
                                                       tools={resources.tools} toolsLoaded={resources.toolsLoaded}
                                                       models={resources.models} profiles={resources.profiles} report={report}
                                                       onClose={() => setPane(null)} onName={renameNode} onKind={editKind} onDelete={requestDelete}/>
                                    )}
                                </Div>
                            </>
                        ) : view === 'runs' ? (
                            <RunsPanel blueprintId={document.blueprint.id} runs={runs}
                                       onOpen={target => navigate(`/blueprints/${target}/runs`)}/>
                        ) : view === 'history' ? (
                            <HistoryPanel key={document.blueprint.id} blueprintId={document.blueprint.id} revision={document.blueprint.revision}/>
                        ) : (
                            <DocumentSettings view={view} name={document.blueprint.name}
                                              schedule={document.blueprint.schedule} nextDue={runs.nextDue} locked={!!busy}
                                              onName={renameDocument} onSchedule={changeSchedule}/>
                        )}
                        {report ? (
                            <section className="blueprint-editor__validation">
                                <strong>{report.valid ? 'Validation passed' : 'Review the blueprint'}</strong>
                                {report.diagnostics.map((item, index) => (
                                    <Button key={index} variant="secondary" disabled={!item.node_id}
                                            onClicked={() => {
                                                select(item.node_id)
                                                navigate(path('graph'), {replace: true})
                                            }}>
                                        {item.severity}: {item.message}
                                    </Button>
                                ))}
                            </section>
                        ) : null}
                    </>
                ) : null}
                {runs.live?.state.pending_tool ? (
                    <RunPermission pending={runs.live.state.pending_tool} busy={runs.acting}
                                   onAnswer={allowance => { void runs.answer(allowance) }} onStop={() => { void runs.stop() }}/>
                ) : null}
                <footer className="blueprint-editor__status">
                    <span>{node?.name ?? (document ? `${document.workflow.nodes.length} nodes` : '')}</span>
                    <span>
                        {runs.live && document
                            ? runs.live.run.revision === document.blueprint.revision
                                ? `${statusText(runs.live.run.status)} · ${progressText(runs.live.run.progress)}${runs.live.state.current_node ? ` · ${runs.live.state.current_node}` : ''}`
                                : `Run is revision ${runs.live.run.revision}; the open graph is revision ${document.blueprint.revision}`
                            : ''}
                    </span>
                </footer>
            </Frame>
            <ConfirmPopup open={removing !== null} title={removing?.type === 'node' ? 'Delete node' : 'Delete connection'}
                          message={removing?.type === 'node'
                              ? `Delete "${document?.workflow.nodes.find(item => item.id === removing.id)?.name ?? removing.id}" and its control connections? ${document ? nodeReferences(document.workflow, removing.id) : 0} input bindings will be reset. Undo can restore them.`
                              : removing?.type === 'data' ? 'Delete this data connection? Its binding will become null. Undo can restore it.'
                                  : 'Delete this control connection? Undo can restore it.'}
                          confirmLabel="Delete" onClose={graph.closeRemoval} onConfirm={deleteSelection}/>
            <UnsavedChangesPopup open={confirm !== null} saving={busy === 'saving'} saveDisabled={conflict || !!busy}
                                 onClose={() => setConfirm(null)} onDiscard={discard}
                                 onSave={() => { void saveAndContinue() }}/>
            <RunStartPopup open={runConfirm} saving={busy === 'saving'} saveDisabled={conflict || !!busy}
                           onClose={() => setRunConfirm(false)}
                           onRunSaved={() => { setRunConfirm(false); if (document) void runs.start(document.blueprint.revision) }}
                           onSaveAndRun={() => { void saveAndRun() }}/>
        </Div>
    )
}
export default BlueprintEditor
