import {createPortal} from 'react-dom'
import {useBlueprintPalette} from '../../hooks/blueprint-editor/use-blueprint-palette'
import type {GraphPoint, NodeChoice} from '../../utils/blueprint-graph'
import type {NodeCatalog as Catalog} from '../../api/blueprints/types'
import {Button, Div, Input, Label} from '../primitives'

interface NodeCatalogProps {
    catalog: Catalog
    query: string
    onQuery: (query: string) => void
    onClose: () => void
    onAdd: (choice: NodeChoice, point?: GraphPoint) => void
    onDrag: () => void
    locked: boolean
    hasEntry: boolean
}

export const NodeCatalog = ({catalog, query, onQuery, onClose, onAdd, onDrag, locked, hasEntry}: NodeCatalogProps) => {
    const palette = useBlueprintPalette(onAdd, onDrag, locked)
    const search = query.toLowerCase()
    const nodes = catalog.nodes.filter(item => item.kind !== 'plugin' && item.title.toLowerCase().includes(search))
    const plugins = catalog.plugin_nodes.filter(item => `${item.title} ${item.plugin_name}`.toLowerCase().includes(search))

    return (
        <aside className="blueprint-editor__catalog">
            <Div className="blueprint-editor__pane-heading">
                <span>Node catalog</span>
                <Button variant="secondary" onClicked={onClose}>×</Button>
            </Div>
            <Input text={query} placeholder="Find a node" onChanged={onQuery}/>
            <Div className="blueprint-editor__catalog-list">
                <Label className="section-heading" text="Built in"/>
                {nodes.map(item => (
                    <div key={item.kind} onPointerDown={event => {
                        if (!(item.kind === 'entry' && hasEntry)) palette.start(event, {type: 'builtin', definition: item})
                    }}>
                        <Button variant="secondary" className="blueprint-editor__catalog-item"
                                disabled={locked || (item.kind === 'entry' && hasEntry)}
                                onClicked={event => palette.click({type: 'builtin', definition: item}, event.detail)}>
                            {item.title}
                        </Button>
                    </div>
                ))}
                {catalog.plugin_nodes.length ? <Label className="section-heading" text="Plugins"/> : null}
                {plugins.map(item => (
                    <div key={`${item.plugin_name}/${item.plugin_subname}/${item.node_type}/${item.version}`}
                         onPointerDown={event => palette.start(event, {type: 'plugin', definition: item})}>
                        <Button variant="secondary" className="blueprint-editor__catalog-item" disabled={locked}
                                onClicked={event => palette.click({type: 'plugin', definition: item}, event.detail)}>
                            {item.title}
                            <small>{item.plugin_name} · v{item.version}</small>
                        </Button>
                    </div>
                ))}
            </Div>
            {palette.ghost ? createPortal(
                <Div className="blueprint-editor__drag-ghost"
                     style={{left: palette.ghost.point.x + 12, top: palette.ghost.point.y + 12}}>
                    {palette.ghost.title}
                </Div>, document.body,
            ) : null}
        </aside>
    )
}
