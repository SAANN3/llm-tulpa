import {useEffect, useMemo, useRef, useState, type CSSProperties} from 'react'
import axios from 'axios'

import '../styles/model-picker.scss'
import {importFiles} from '../api/llm/import'
import {listLocalFiles} from '../api/llm/local-files'
import type {LocalModel} from '../api/llm/models'
import {listModels} from '../api/llm/models'
import {startPull} from '../api/llm/pull'
import {fetchCatalog} from '../api/llm/catalog'
import type {Catalog, LocalFile, LocalFiles, ModelTask} from '../api/llm/types'
import {useAuth} from '../context/use-auth.ts'
import {useModelTasks} from '../hooks/use-model-tasks.ts'
import {formatBytes} from '../utils/format-bytes.ts'
import {modelRequirements} from '../utils/model-info'
import {Button, Checkbox, Div, Input, Label, Select} from './primitives'

export interface ModelPickerProps {
    selected?: string | null
    onSelect: (name: string) => void
}

const NO_PROJECTOR = 'no vision'

const baseName = (tag: string): string => tag.split(':')[0]

/** The backend's own explanation when it gave one (`{error}`), else a generic fallback */
const reason = (e: unknown, fallback: string): string =>
    (axios.isAxiosError(e) && (e.response?.data as { error?: string } | undefined)?.error) || fallback

const catalogMeta = (m: Catalog['models'][number]): string =>
    [m.capabilities.join(', '), m.sizes.join(' '), m.pulls ? `${m.pulls} pulls` : ''].filter(Boolean).join(' · ')

/**
 * Pick the model to chat with. Anyone can choose among the installed models; the owner can
 * also add more — pull from Ollama's library or Hugging Face, or import `.gguf` files that are
 * already on disk (several at once) — and watch those run in the background.
 */
export const ModelPicker = ({selected, onSelect}: ModelPickerProps) => {
    const {user} = useAuth()
    const isOwner = user?.role === 'owner'

    const [local, setLocal] = useState<LocalModel[]>([])
    const [catalog, setCatalog] = useState<Catalog | null>(null)
    const [files, setFiles] = useState<LocalFiles | null>(null)
    const [search, setSearch] = useState('')
    const [error, setError] = useState<string | null>(null)
    const [checked, setChecked] = useState<Set<string>>(new Set())
    const [projectors, setProjectors] = useState<Record<string, string>>({})
    const [importing, setImporting] = useState(false)
    // Tasks worth showing: the ones this picker started or saw running, not last week's.
    const [shown, setShown] = useState<Set<number>>(new Set())
    // The pull whose model should become the selection once it lands.
    const selectWhenDone = useRef<number | null>(null)

    const refreshLocal = () => listModels().then(setLocal).catch(() => setError('Could not reach Ollama.'))

    const {tasks, refresh: refreshTasks} = useModelTasks(isOwner, (task) => {
        listModels()
            .then((list) => {
                setLocal(list)
                if (task.state === 'done' && selectWhenDone.current === task.id) {
                    const installed = list.find((m) => m.name === task.model || m.name === `${task.model}:latest`)
                    if (installed) onSelect(installed.name)
                }
            })
            .catch(() => setError('Could not reach Ollama.'))
    })

    useEffect(() => {
        refreshLocal()
        if (!isOwner) return
        fetchCatalog().then(setCatalog).catch(() => setCatalog(null))
        listLocalFiles().then(setFiles).catch(() => setFiles(null))
    }, [isOwner])

    // Remember every task seen running so its outcome stays visible after it finishes.
    useEffect(() => {
        const running = tasks.filter((t) => t.state === 'running').map((t) => t.id)
        if (running.some((id) => !shown.has(id))) setShown((prev) => new Set([...prev, ...running]))
    }, [tasks, shown])

    const installedBases = useMemo(() => new Set(local.map((m) => baseName(m.name))), [local])
    const query = search.trim().toLowerCase()

    const filteredLocal = local.filter((m) => m.name.toLowerCase().includes(query))
    const catalogModels = catalog?.models ?? []
    const filteredCatalog = catalogModels
        .filter((m) => !installedBases.has(baseName(m.name)))
        .filter((m) => m.name.toLowerCase().includes(query) || m.description?.toLowerCase().includes(query))

    // Projectors aren't models on their own: they only appear as the vision option of a model.
    const modelFiles = (files?.files ?? []).filter((f) => f.kind !== 'projector')
    const filteredFiles = modelFiles.filter((f) => f.path.toLowerCase().includes(query))

    const exactTag = search.trim()
    const showExactPull =
        isOwner &&
        exactTag.length > 0 &&
        !local.some((m) => m.name === exactTag) &&
        !catalogModels.some((m) => m.name === exactTag)

    const onPull = async (name: string) => {
        setError(null)
        try {
            const task = await startPull(name)
            selectWhenDone.current = task.id
            setShown((prev) => new Set([...prev, task.id]))
            await refreshTasks()
        } catch (e) {
            setError(reason(e, `Could not start pulling "${name}".`))
        }
    }

    const toggleFile = (file: LocalFile, on: boolean) => {
        setChecked((prev) => {
            const next = new Set(prev)
            if (on) next.add(file.path)
            else next.delete(file.path)
            return next
        })
        // Ticking a model preselects its projector when the backend found a clear match.
        if (on && file.suggested_projector) {
            setProjectors((prev) => (file.path in prev ? prev : {...prev, [file.path]: file.suggested_projector!}))
        }
    }

    const onImport = async () => {
        setImporting(true)
        setError(null)
        try {
            const started = await importFiles(
                [...checked].map((file) => {
                    const projector = projectors[file]
                    return projector && projector !== NO_PROJECTOR ? {file, projector} : {file}
                }),
            )
            setShown((prev) => new Set([...prev, ...started.map((t) => t.id)]))
            setChecked(new Set())
            await refreshTasks()
        } catch (e) {
            setError(reason(e, 'Could not start the import.'))
        } finally {
            setImporting(false)
        }
    }

    const visibleTasks = tasks.filter((t) => t.state === 'running' || shown.has(t.id))

    return (
        <Div className="model-picker">
            <Input className="model-picker__search" text={search} onChanged={setSearch}
                   placeholder={isOwner ? 'Search, or type a tag (qwen3:8b) or hf.co/user/repo' : 'Search installed models'}/>

            {error ? <Label variant="secondary" className="model-picker__error" text={error}/> : null}

            <Div className="model-picker__list">
                {visibleTasks.length > 0 ? (
                    <Label variant="secondary" className="model-picker__section" text="In progress"/>
                ) : null}
                {visibleTasks.map((task) => <TaskRow key={task.id} task={task}/>)}

                {showExactPull ? (
                    <Div className="model-picker__row">
                        <Div className="model-picker__info">
                            <Label className="model-picker__name" text={exactTag}/>
                            <Label variant="secondary" className="model-picker__meta"
                                   text="Pull this exact name — a library tag or a Hugging Face GGUF"/>
                        </Div>
                        <Button variant="secondary" text="Pull" onClicked={() => onPull(exactTag)}/>
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

                {isOwner && files ? (
                    <>
                        <Label variant="secondary" className="model-picker__section" text="Local files"/>
                        {!files.configured ? (
                            <Label variant="secondary" className="model-picker__meta"
                                   text="No model folder configured — set model_dir in the backend's settings.json to import .gguf files."/>
                        ) : filteredFiles.length === 0 ? (
                            <Label variant="secondary" className="model-picker__meta"
                                   text={modelFiles.length === 0 ? 'No .gguf files in the model folder.' : 'No file matches.'}/>
                        ) : null}
                        {filteredFiles.map((f) => f.kind === 'invalid' ? (
                            <Div key={f.path} className="model-picker__row model-picker__row--invalid">
                                <Div className="model-picker__info">
                                    <Label className="model-picker__name" text={f.path}/>
                                    <Label variant="secondary" className="model-picker__meta model-picker__meta--error"
                                           text={`Not a usable GGUF file — ${f.error ?? 'unreadable'}`}/>
                                </Div>
                            </Div>
                        ) : (
                            <Div key={f.path} className="model-picker__row">
                                <Checkbox toggled={checked.has(f.path)} onToggled={(on) => toggleFile(f, on)}/>
                                <Div className="model-picker__info">
                                    <Label className="model-picker__name" text={f.path}/>
                                    <Label variant="secondary" className="model-picker__meta" text={formatBytes(f.size_bytes)}/>
                                </Div>
                                {f.compatible_projectors.length > 0 && checked.has(f.path) ? (
                                    <Select className="model-picker__projector"
                                            values={[NO_PROJECTOR, ...f.compatible_projectors]}
                                            selected={projectors[f.path] ?? NO_PROJECTOR}
                                            onChosen={(value) => setProjectors((prev) => ({...prev, [f.path]: value}))}/>
                                ) : null}
                            </Div>
                        ))}
                        {checked.size > 0 ? (
                            <Button text={importing ? 'Starting…' : `Import ${checked.size} selected`}
                                    onClicked={onImport} disabled={importing}/>
                        ) : null}
                    </>
                ) : null}

                {isOwner && filteredCatalog.length > 0 ? (
                    <Label variant="secondary" className="model-picker__section"
                           text={catalog?.live === false ? 'Available to pull (offline list)' : 'Available to pull'}/>
                ) : null}
                {isOwner && filteredCatalog.map((m) => (
                    <Div key={m.name} className="model-picker__row">
                        <Div className="model-picker__info">
                            <Label className="model-picker__name" text={m.name}/>
                            {m.description ? (
                                <Label variant="secondary" className="model-picker__meta" text={m.description}/>
                            ) : null}
                            {catalogMeta(m) ? (
                                <Label variant="secondary" className="model-picker__meta" text={catalogMeta(m)}/>
                            ) : null}
                        </Div>
                        <Button variant="secondary" text="Pull" onClicked={() => onPull(m.name)}/>
                    </Div>
                ))}
            </Div>
        </Div>
    )
};

/** One pull/import: what it's doing, a progress bar while it runs, the reason if it failed */
const TaskRow = ({task}: { task: ModelTask }) => {
    const fraction = task.total_bytes > 0 ? Math.min(1, task.completed_bytes / task.total_bytes) : 0
    const detail =
        task.state === 'failed'
            ? task.error ?? 'failed'
            : task.state === 'done'
                ? 'done'
                : `${task.phase}${task.total_bytes > 0 ? ` · ${formatBytes(task.completed_bytes)} / ${formatBytes(task.total_bytes)}` : ''}`

    return (
        <Div className="model-picker__row model-picker__row--task">
            <Div className="model-picker__info">
                <Label className="model-picker__name" text={`${task.kind === 'pull' ? 'Pulling' : 'Importing'} ${task.model}`}/>
                <Label variant="secondary" className={`model-picker__meta${task.state === 'failed' ? ' model-picker__meta--error' : ''}`}
                       text={detail}/>
                {task.state === 'running' ? (
                    <Div className="model-picker__bar">
                        <Div className="model-picker__bar-fill" style={{'--fraction': fraction} as CSSProperties}/>
                    </Div>
                ) : null}
            </Div>
        </Div>
    )
};
