import {useEffect, useRef, useState} from 'react'
import {Navigate, useNavigate, useParams} from 'react-router-dom'
import {Layout} from 'pixelarticons/react'
import type {Schedule} from '../api/blueprints/types'
import {Frame} from '../components/frame'
import {Button, Div, Label} from '../components/primitives'
import {UnsavedChangesPopup} from '../components/popups/unsaved-changes-popup'
import {keyBelongsToFocus, useKeyLayer} from '../hooks/use-key-layer'
import {useDocumentTitle} from '../hooks/use-document-title'
import {useBlueprintDocument} from '../hooks/use-blueprint-document'
import {EditorMenubar} from '../components/blueprint-editor/editor-menubar'
import {NodeCatalog} from '../components/blueprint-editor/node-catalog'
import {GraphCanvas} from '../components/blueprint-editor/graph-canvas'
import {NodeInspector} from '../components/blueprint-editor/node-inspector'
import {DocumentSettings} from '../components/blueprint-editor/document-settings'
import {EDITOR_VIEWS} from '../utils/blueprint-views'
import '../styles/blueprint-editor.scss'

const BlueprintEditor = () => {
    const {id, view = 'graph'} = useParams()
    const navigate = useNavigate()
    const editor = useBlueprintDocument(Number(id))
    const {document, catalog, dirty, busy, loading, error, conflict, report} = editor
    useDocumentTitle(document?.blueprint.name ?? 'Blueprint')
    const [selected, setSelected] = useState<string | null>(null)
    const [query, setQuery] = useState('')
    const [pane, setPane] = useState<'catalog' | 'inspector' | null>(null)
    const [paletteVisible, setPaletteVisible] = useState(true)
    const [inspectorVisible, setInspectorVisible] = useState(true)
    const [confirm, setConfirm] = useState<'back' | 'reload' | null>(null)
    const fitRef = useRef<(() => void) | null>(null)
    const node = document?.workflow.nodes.find(item => item.id === selected)
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
    const select = (nodeId: string | null) => {
        setSelected(nodeId)
        if (nodeId) {
            setPane('inspector')
            setInspectorVisible(true)
        }
    }
    const changeView = (next: string) => {
        navigate(path(next), {replace: true})
        setPane(null)
    }
    const moveNode = (nodeId: string, position: {x: number; y: number}) => {
        editor.edit(value => ({
            ...value,
            workflow: {
                ...value.workflow,
                nodes: value.workflow.nodes.map(item => item.id === nodeId ? {...item, position} : item),
            },
        }))
    }
    const renameNode = (name: string) => {
        editor.edit(value => ({
            ...value,
            workflow: {
                ...value.workflow,
                nodes: value.workflow.nodes.map(item => item.id === node?.id ? {...item, name} : item),
            },
        }))
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
            setSelected(null)
            setPane(null)
            return true
        }
        if (['1', '2', '3'].includes(event.key)) {
            navigate(path(EDITOR_VIEWS[Number(event.key) - 1].id), {replace: true})
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
        ],
        view: [
            {label: 'Node catalog', toggled: paletteVisible, onSelect: () => setPaletteVisible(value => !value)},
            {label: 'Node inspector', toggled: inspectorVisible, onSelect: () => setInspectorVisible(value => !value)},
            {label: 'Fit graph', onSelect: () => fitRef.current?.()},
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
                    </Div>
                </Div>
                {error ? (
                    <Div className="blueprint-editor__error">
                        <Label text={error}/>
                        <Button variant="secondary" disabled={!!busy} onClicked={requestReload}>Reload</Button>
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
                                    <NodeCatalog catalog={catalog} query={query} onQuery={setQuery} onClose={() => setPane(null)}/>
                                    <GraphCanvas key={id} workflow={document.workflow} catalog={catalog}
                                                 selected={selected} onSelect={select} locked={!!busy}
                                                 fitRef={fitRef} onMove={moveNode}/>
                                    <NodeInspector node={node} catalog={catalog} locked={!!busy}
                                                   onClose={() => setPane(null)} onName={renameNode}/>
                                </Div>
                            </>
                        ) : (
                            <DocumentSettings view={view} name={document.blueprint.name}
                                              schedule={document.blueprint.schedule} locked={!!busy}
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
                <footer className="blueprint-editor__status">
                    <span>{node?.name ?? (document ? `${document.workflow.nodes.length} nodes` : '')}</span>
                </footer>
            </Frame>
            <UnsavedChangesPopup open={confirm !== null} saving={busy === 'saving'} saveDisabled={conflict || !!busy}
                                 onClose={() => setConfirm(null)} onDiscard={discard}
                                 onSave={() => { void saveAndContinue() }}/>
        </Div>
    )
}
export default BlueprintEditor
