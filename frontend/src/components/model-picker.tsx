import {useEffect, useMemo, useState} from 'react'

import '../styles/model-picker.scss'
import {fetchCatalog} from '../api/llm/catalog'
import type {LocalModel} from '../api/llm/models'
import {listModels} from '../api/llm/models'
import {pullModel} from '../api/llm/pull'
import {Button, Div, Input, Label} from './primitives'
import {FALLBACK_CATALOG, modelRequirements, parseCatalog, type CatalogModel} from '../utils/model-info'

export interface ModelPickerProps {
    selected?: string | null
    onSelect: (name: string) => void
}

const baseName = (tag: string): string => tag.split(':')[0]

/**
 * Browse + pick + pull an Ollama model
 */
export const ModelPicker = ({selected, onSelect}: ModelPickerProps) => {
    const [local, setLocal] = useState<LocalModel[]>([])
    const [catalog, setCatalog] = useState<CatalogModel[]>([])
    const [search, setSearch] = useState('')
    const [pulling, setPulling] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)

    const refreshLocal = () => listModels().then(setLocal).catch(() => setError('Could not reach Ollama.'))

    useEffect(() => {
        refreshLocal()
        fetchCatalog()
            .then((html) => setCatalog(html ? parseCatalog(html) : FALLBACK_CATALOG))
            .catch(() => setCatalog(FALLBACK_CATALOG))
    }, [])

    const installedBases = useMemo(() => new Set(local.map((m) => baseName(m.name))), [local])
    const query = search.trim().toLowerCase()

    const filteredLocal = local.filter((m) => m.name.toLowerCase().includes(query))
    const filteredCatalog = catalog
        .filter((m) => !installedBases.has(baseName(m.name)))
        .filter((m) => m.name.toLowerCase().includes(query) || m.description?.toLowerCase().includes(query))

    const exactTag = search.trim()
    const showExactPull =
        exactTag.length > 0 &&
        !local.some((m) => m.name === exactTag) &&
        !catalog.some((m) => m.name === exactTag)

    const onPull = async (name: string) => {
        setPulling(name)
        setError(null)
        try {
            await pullModel(name)
            await refreshLocal()
            onSelect(name)
        } catch {
            setError(`Could not pull "${name}".`)
        } finally {
            setPulling(null)
        }
    }

    return (
        <Div className="model-picker">
            <Input className="model-picker__search" text={search} onChanged={setSearch}
                   placeholder="Search or type a tag (e.g. qwen2.5:0.5b)"/>

            {error ? <Label variant="secondary" className="model-picker__error" text={error}/> : null}

            <Div className="model-picker__list">
                {showExactPull ? (
                    <Div className="model-picker__row">
                        <Div className="model-picker__info">
                            <Label className="model-picker__name" text={exactTag}/>
                            <Label variant="secondary" className="model-picker__meta" text="Pull this exact tag"/>
                        </Div>
                        <Button variant="secondary" text={pulling === exactTag ? 'Pulling…' : 'Pull'}
                                onClicked={() => onPull(exactTag)} disabled={pulling != null}/>
                    </Div>
                ) : null}

                {filteredLocal.length > 0 ? (
                    <Label variant="secondary" className="model-picker__section" text="Installed"/>
                ) : null}
                {filteredLocal.map((m) => {
                    const isSelected = m.name === selected
                    return (
                        <Div key={m.name} className={`model-picker__row${isSelected ? ' model-picker__row--active' : ''}`}>
                            <Div className="model-picker__info">
                                <Label className="model-picker__name" text={m.name}/>
                                <Label variant="secondary" className="model-picker__meta"
                                       text={modelRequirements(m) || 'installed'}/>
                            </Div>
                            {isSelected ? (
                                <Label variant="secondary" className="model-picker__active-tag" text="active"/>
                            ) : (
                                <Button variant="secondary" text="Select" onClicked={() => onSelect(m.name)}/>
                            )}
                        </Div>
                    )
                })}

                {filteredCatalog.length > 0 ? (
                    <Label variant="secondary" className="model-picker__section" text="Available to pull"/>
                ) : null}
                {filteredCatalog.map((m) => (
                    <Div key={m.name} className="model-picker__row">
                        <Div className="model-picker__info">
                            <Label className="model-picker__name" text={m.name}/>
                            {m.description ? (
                                <Label variant="secondary" className="model-picker__meta" text={m.description}/>
                            ) : null}
                        </Div>
                        <Button variant="secondary" text={pulling === m.name ? 'Pulling…' : 'Pull'}
                                onClicked={() => onPull(m.name)} disabled={pulling != null}/>
                    </Div>
                ))}
            </Div>
        </Div>
    )
};
