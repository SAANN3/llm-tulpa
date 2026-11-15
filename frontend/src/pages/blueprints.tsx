import {useCallback, useRef, useState} from 'react'
import {flushSync} from 'react-dom'
import {useNavigate, useSearchParams} from 'react-router-dom'
import {blueprintName} from '../api/prompts/blueprint-name'
import {NewNamedPopup} from '../components/popups/new-named-popup.tsx'
import {createBlueprint} from '../api/blueprints/create'
import {deleteBlueprint} from '../api/blueprints/delete'
import {updateBlueprint} from '../api/blueprints/update'
import type {Blueprint} from '../api/blueprints/types'
import {LazyList, type LazyListHandle} from '../components/lazy-list.tsx'
import {ConfirmPopup} from '../components/popups/base/confirm-popup.tsx'
import {InputPopup} from '../components/popups/base/input-popup.tsx'
import {Button, Div, Input, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {keyBelongsToFocus, useKeyLayer} from '../hooks/use-key-layer.ts'
import {useBlueprints} from '../hooks/use-blueprints.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import '../styles/blueprints.scss'

const Blueprints = () => {
    useDocumentTitle('Blueprints')
    const navigate = useNavigate()
    const [search] = useSearchParams()
    const query = search.get('q') ?? ''
    const list = useRef<LazyListHandle>(null)
    const firstRow = useRef<HTMLDivElement>(null)
    const updateList = useCallback((update: () => void, reset: boolean) => {
        if (reset) {
            list.current?.jumpToTop()
            flushSync(update)
        } else if (list.current && firstRow.current) {
            // LazyList normally follows the bottom; appending older rows should leave the reader in place.
            list.current.preserveViewportPosition(firstRow.current, () => flushSync(update))
        } else update()
    }, [])
    const {result, loading, error, busy, mutate, refresh, loadOlder, clearError} = useBlueprints(query, updateList)
    const [creating, setCreating] = useState(false)
    const [renaming, setRenaming] = useState<Blueprint | null>(null)
    const [removing, setRemoving] = useState<Blueprint | null>(null)
    const path = (q = query) => `/blueprints${q ? `?${new URLSearchParams({q})}` : ''}`

    useKeyLayer(true, (event) => {
        if (event.key !== 'n' || event.ctrlKey || event.metaKey || event.altKey || keyBelongsToFocus(event) || busy) return false
        setCreating(true)
        return true
    })

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="vbox blueprints">
                <Div className="blueprints__header">
                    <TypewriterLabel className="mono blueprints__title" text="~/blueprints" charIntervalMs={30}/>
                    <Button variant="secondary" text="+ New blueprint" disabled={busy} onClicked={() => setCreating(true)}/>
                </Div>
                <Input className="blueprints__search" text={query} placeholder="Search blueprints" onChanged={(q) => {
                    clearError()
                    navigate(path(q), {replace: true})
                }}/>
                {error ? <Div className="blueprints__error">
                    <Label text={error}/>
                    <Button text="Reload" disabled={loading || busy} onClicked={() => { clearError(); refresh() }}/>
                </Div> : null}
                <LazyList ref={list} onBottomReached={loadOlder} className="blueprints__list">
                    {result?.items.length === 0 ? <Label text={query.trim() ? 'No matching blueprints.' : 'No blueprints yet. Create one to get started.'}/> : null}
                    {result?.items.map((blueprint, index) => (
                        <Div key={blueprint.id} ref={index === 0 ? firstRow : undefined} className="blueprints__row">
                            <Div className="blueprints__info">
                                <Label className="blueprints__name" text={blueprint.name}/>
                                <Label variant="secondary" className="blueprints__date"
                                       text={`Created ${new Date(blueprint.created_at).toLocaleString()} · Updated ${new Date(blueprint.updated_at).toLocaleString()}`}/>
                            </Div>
                            <Div className="blueprints__actions">
                                <Button text="Rename" disabled={busy} onClicked={() => setRenaming(blueprint)}/>
                                <Button text="Delete" disabled={busy} onClicked={() => setRemoving(blueprint)}/>
                            </Div>
                        </Div>
                    ))}
                </LazyList>
                <Label variant="secondary" text={result ? `${result.items.length} of ${result.total} blueprints` : loading ? 'Loading blueprints...' : ' '}/>
            </Div>
            <NewNamedPopup open={creating} title="New blueprint" resource="blueprint"
                           create={createBlueprint} generateName={blueprintName}
                           onClose={() => setCreating(false)} onCreated={() => {
                               setCreating(false)
                               clearError()
                               refresh()
                               navigate(path(''), {replace: true})
                           }}/>
            <InputPopup open={renaming !== null} title="Rename blueprint" value={renaming?.name}
                        onClose={() => setRenaming(null)} onSubmit={(name) => {
                            if (renaming) void mutate(() => updateBlueprint(renaming, name))
                        }}/>
            <ConfirmPopup open={removing !== null} title="Delete blueprint" confirmLabel="Delete"
                          message={`Delete "${removing?.name ?? ''}" and its run history? This cannot be undone.`}
                          onClose={() => setRemoving(null)} onConfirm={() => {
                              if (removing) void mutate(() => deleteBlueprint(removing))
                          }}/>
        </Div>
    )
};

export default Blueprints
