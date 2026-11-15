import type {NodeCatalog as Catalog} from '../../api/blueprints/types'
import {Button, Div, Input, Label} from '../primitives'

interface NodeCatalogProps {
    catalog: Catalog
    query: string
    onQuery: (query: string) => void
    onClose: () => void
}

export const NodeCatalog = ({catalog, query, onQuery, onClose}: NodeCatalogProps) => {
    const search = query.toLowerCase()
    const nodes = catalog.nodes.filter(item => item.title.toLowerCase().includes(search))
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
                    <Div key={item.kind} className="blueprint-editor__catalog-item">
                        {item.title}
                    </Div>
                ))}
                {catalog.plugin_nodes.length ? <Label className="section-heading" text="Plugins"/> : null}
                {plugins.map(item => (
                    <Div key={`${item.plugin_name}/${item.plugin_subname}/${item.node_type}/${item.version}`}
                         className="blueprint-editor__catalog-item">
                        {item.title}
                        <small>{item.plugin_name} · v{item.version}</small>
                    </Div>
                ))}
            </Div>
        </aside>
    )
}
