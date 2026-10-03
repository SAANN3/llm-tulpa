import {useEffect, useState, type CSSProperties} from 'react'
import '../../styles/model-picker.scss'
import {startHfDownload} from '../../api/hf/download'
import {listHfFiles} from '../../api/hf/files'
import {searchHf} from '../../api/hf/search'
import {listHfTasks} from '../../api/hf/tasks'
import type {HfFile, HfRepo, HfTask} from '../../api/hf/types'
import {errorReason} from '../../utils/error-reason.ts'
import {formatBytes} from '../../utils/format-bytes.ts'
import {Button, Checkbox, Div, Input, Label} from '../primitives'

export interface HfPanelProps {
    /** Called when a download finishes, so the model list can pick the file up */
    onDownloaded: () => void
}

const POLL_MS = 1500

/** Searches Hugging Face for GGUF models and downloads their files into the model folder. A gated
 * repository is hidden until asked for, and says what it needs (a token in the settings). */
export const HfPanel = ({onDownloaded}: HfPanelProps) => {
    const [query, setQuery] = useState('')
    const [repos, setRepos] = useState<HfRepo[] | null>(null)
    const [hasToken, setHasToken] = useState(false)
    const [showGated, setShowGated] = useState(false)
    const [open, setOpen] = useState<string | null>(null)
    const [files, setFiles] = useState<HfFile[]>([])
    const [tasks, setTasks] = useState<HfTask[]>([])
    const [error, setError] = useState<string | null>(null)

    const refreshTasks = () => listHfTasks().then((next) => {
        setTasks((prev) => {
            if (prev.some((t) => t.state === 'running') && next.every((t) => t.state !== 'running')) onDownloaded()
            return next
        })
    }).catch(() => undefined)

    useEffect(() => {
        void refreshTasks()
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    const running = tasks.some((t) => t.state === 'running')
    useEffect(() => {
        if (!running) return
        const id = setInterval(() => void refreshTasks(), POLL_MS)
        return () => clearInterval(id)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [running])

    const search = async () => {
        setError(null)
        setOpen(null)
        try {
            const result = await searchHf(query)
            setRepos(result.repos)
            setHasToken(result.has_token)
        } catch (e) {
            setError(errorReason(e, 'The search failed.'))
        }
    }

    const openRepo = async (repo: HfRepo) => {
        setError(null)
        if (open === repo.id) {
            setOpen(null)
            return
        }
        try {
            setFiles(await listHfFiles(repo.id))
            setOpen(repo.id)
        } catch (e) {
            setError(errorReason(e, 'Could not list the files.'))
        }
    }

    const download = async (repo: string, file: string) => {
        setError(null)
        try {
            await startHfDownload(repo, file)
            await refreshTasks()
        } catch (e) {
            setError(errorReason(e, 'Could not start the download.'))
        }
    }

    const shown = (repos ?? []).filter((r) => showGated || !r.gated)
    const hidden = (repos ?? []).length - shown.length

    return (
        <Div className="models__section">
            <Div className="models__toolbar">
                <Input text={query} onChanged={setQuery} placeholder="Search Hugging Face for GGUF models"
                       onKeyDown={(e) => e.key === 'Enter' && query.trim() && void search()}/>
                <Button text="Search" disabled={!query.trim()} onClicked={() => void search()}/>
            </Div>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}

            {tasks.map((t) => (
                <Div key={t.id} className="models__preset">
                    <Div className="models__profile-main">
                        <Label className="models__profile-name" text={t.local_path}/>
                        <Label variant="secondary" className={t.state === 'failed' ? 'models__error' : 'models__meta'}
                               text={t.state === 'failed' ? t.error ?? 'failed' : t.state === 'done' ? 'downloaded — add it on the Models tab'
                                   : `${t.phase} · ${formatBytes(t.completed_bytes)} / ${formatBytes(t.total_bytes)}`}/>
                        {t.state === 'running' ? (
                            <Div className="model-picker__bar">
                                <Div className="model-picker__bar-fill"
                                     style={{'--fraction': t.total_bytes > 0 ? t.completed_bytes / t.total_bytes : 0} as CSSProperties}/>
                            </Div>
                        ) : null}
                    </Div>
                </Div>
            ))}

            {repos != null ? (
                <Div className="field__row">
                    <Label className="field__row-label" text="Show gated models"/>
                    <Checkbox toggled={showGated} onToggled={setShowGated}/>
                </Div>
            ) : null}
            {hidden > 0 && !showGated ? (
                <Label variant="secondary" className="models__meta" text={`${hidden} gated model${hidden === 1 ? '' : 's'} hidden.`}/>
            ) : null}
            {repos?.length === 0 ? <Label variant="secondary" text="Nothing found."/> : null}

            {shown.map((repo) => (
                <Div key={repo.id} className="models__model">
                    <Div className="models__model-head">
                        <Div className="models__profile-main">
                            <Label className="models__model-name" text={repo.id}/>
                            <Label variant="secondary" className="models__meta"
                                   text={`${repo.downloads} downloads · ${repo.likes} likes`}/>
                            {repo.gated ? (
                                <Label variant="secondary" className={hasToken ? 'models__meta' : 'models__error'}
                                       text={hasToken ? 'Gated: your token is used for the download.'
                                           : 'This model is gated by Hugging Face, to download it you need to set up your token in the settings.'}/>
                            ) : null}
                        </Div>
                        <Button variant="secondary" text={open === repo.id ? 'Hide files' : 'Files'} onClicked={() => void openRepo(repo)}/>
                    </Div>
                    {open === repo.id ? files.map((f) => (
                        <Div key={f.path} className="models__profile">
                            <Div className="models__profile-main">
                                <Label className="models__profile-name" text={f.path}/>
                                <Label variant="secondary" className="models__meta"
                                       text={`${formatBytes(f.size_bytes)}${f.projector ? ' · vision projector' : ''}`}/>
                            </Div>
                            <Button variant="secondary" text="Download" onClicked={() => void download(repo.id, f.path)}/>
                        </Div>
                    )) : null}
                </Div>
            ))}
        </Div>
    )
};
