import {useEffect, useRef, useState, type CSSProperties} from 'react'
import '../../styles/model-picker.scss'
import '../../styles/models.scss'
import {getModelFolder, type ModelFolder} from '../../api/runtime/folder'
import {startHfDownload} from '../../api/hf/download'
import {listHfFiles} from '../../api/hf/files'
import {searchHf, type HfSort} from '../../api/hf/search'
import {listHfTasks} from '../../api/hf/tasks'
import type {HfFile, HfRepo, HfTask} from '../../api/hf/types'
import {errorReason} from '../../utils/error-reason.ts'
import {formatBytes} from '../../utils/format-bytes.ts'
import {Button, Checkbox, Div, Input, Label} from '../primitives'

export interface HfPanelProps {
    /** Called when a download finishes, so the model list can pick the file up */
    onDownloaded: () => void
    /** What a finished download says about where to find the file next; by default, the Models tab */
    doneHint?: string
    /** Told whenever a download starts or ends, for a caller that must not be left while one runs */
    onRunningChange?: (running: boolean) => void
}

/** The orders a search can be shown in, as the words above the list */
const SORTS: { sort: HfSort; label: string }[] = [
    {sort: 'downloads', label: 'Most downloaded'},
    {sort: 'likes', label: 'Most liked'},
    {sort: 'createdAt', label: 'Newest'},
    {sort: 'lastModified', label: 'Recently updated'},
]

const shortDate = (at: string) => new Date(at).toLocaleDateString(undefined, {year: 'numeric', month: 'short', day: 'numeric'})

/** How long typing has to pause before the search runs */
const SEARCH_DEBOUNCE_MS = 400

const POLL_MS = 1500

/** Left free on the disk after a download; the backend refuses a download that would eat into it */
const DISK_MARGIN_BYTES = 512 * 1024 * 1024

/** Searches Hugging Face for GGUF models and downloads their files into the model folder. A gated
 * repository is hidden until asked for, and says what it needs (a token in the settings). */
export const HfPanel = ({onDownloaded, doneHint = 'downloaded — add it on the Models tab', onRunningChange}: HfPanelProps) => {
    const [query, setQuery] = useState('')
    const [repos, setRepos] = useState<HfRepo[] | null>(null)
    const [sort, setSort] = useState<HfSort>('downloads')
    // Where the next page of the search starts; null on the last page
    const [nextCursor, setNextCursor] = useState<string | null>(null)
    const [loadingMore, setLoadingMore] = useState(false)
    const endMarker = useRef<HTMLDivElement>(null)
    // The cursor the list is at, for a page that arrives late to tell whether it still belongs to it
    const cursorNow = useRef<string | null>(null)
    useEffect(() => {
        cursorNow.current = nextCursor
    }, [nextCursor])
    const [hasToken, setHasToken] = useState(false)
    const [showGated, setShowGated] = useState(false)
    const [open, setOpen] = useState<string | null>(null)
    const [files, setFiles] = useState<HfFile[]>([])
    const [tasks, setTasks] = useState<HfTask[]>([])
    const [error, setError] = useState<string | null>(null)
    const [folder, setFolder] = useState<ModelFolder | null>(null)
    // Files whose download was just requested, until the task list shows them: stops a second press
    const [starting, setStarting] = useState<string[]>([])

    // The free space changes as files arrive, so it is read again whenever a download ends or starts
    const refreshFolder = () => getModelFolder().then(setFolder).catch(() => setFolder(null))

    const refreshTasks = () => listHfTasks().then((next) => {
        setTasks((prev) => {
            if (prev.some((t) => t.state === 'running') && next.every((t) => t.state !== 'running')) {
                onDownloaded()
                void refreshFolder()
            }
            return next
        })
    }).catch(() => undefined)

    useEffect(() => {
        void refreshFolder()
        void refreshTasks()
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    const running = tasks.some((t) => t.state === 'running')
    useEffect(() => onRunningChange?.(running), [running, onRunningChange])
    useEffect(() => {
        if (!running) return
        const id = setInterval(() => void refreshTasks(), POLL_MS)
        return () => clearInterval(id)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [running])

    // Searches as the typing pauses (at once for a change of order); an empty query lists every GGUF repository.
    // `stale` drops the answer of a search that a newer keystroke has already replaced.
    useEffect(() => {
        let stale = false
        const timer = setTimeout(() => {
            setError(null)
            searchHf(query.trim(), sort).then((result) => {
                if (stale) return
                setOpen(null)
                setRepos(result.repos)
                setNextCursor(result.next_cursor)
                setHasToken(result.has_token)
            }).catch((e) => {
                if (!stale) setError(errorReason(e, 'The search failed.'))
            })
        }, query.trim() ? SEARCH_DEBOUNCE_MS : 0)
        return () => {
            stale = true
            clearTimeout(timer)
        }
    }, [query, sort])

    // The next page, when the end of the list scrolls into view. A search that changes meanwhile replaces the
    // list (the effect above), and a page that arrives after that is dropped: its cursor no longer matches.
    useEffect(() => {
        const marker = endMarker.current
        if (!marker || nextCursor == null || loadingMore) return
        const watch = new IntersectionObserver((entries) => {
            if (!entries.some((entry) => entry.isIntersecting)) return
            watch.disconnect()
            const cursor = nextCursor
            setLoadingMore(true)
            searchHf(query.trim(), sort, cursor).then((result) => {
                if (cursorNow.current !== cursor) return
                setRepos((prev) => [...(prev ?? []), ...result.repos])
                setNextCursor(result.next_cursor)
            }).catch((e) => setError(errorReason(e, 'Could not load more.'))).finally(() => setLoadingMore(false))
        })
        watch.observe(marker)
        return () => watch.disconnect()
    }, [nextCursor, loadingMore, query, sort])

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

    // The newest task of a file, wherever it was started from: the state of a file's button comes from
    // here, not from the card that is open, so it reads the same after hiding files or searching again
    const taskOf = (repo: string, file: string) => tasks.findLast((t) => t.repo === repo && t.file === file)

    const download = async (repo: string, file: string) => {
        const key = `${repo}/${file}`
        setError(null)
        setStarting((s) => [...s, key])
        try {
            await startHfDownload(repo, file)
            await refreshTasks()
            void refreshFolder()
        } catch (e) {
            setError(errorReason(e, 'Could not start the download.'))
        } finally {
            setStarting((s) => s.filter((k) => k !== key))
        }
    }

    const downloadButton = (repo: string, f: HfFile) => {
        const task = taskOf(repo, f.path)
        const size = formatBytes(f.size_bytes)
        if (task?.state === 'running' || starting.includes(`${repo}/${f.path}`)) {
            const fraction = task && task.total_bytes > 0 ? Math.floor(100 * task.completed_bytes / task.total_bytes) : null
            return <Button variant="secondary" disabled onClicked={() => undefined} text={fraction == null ? 'Downloading…' : `Downloading ${fraction}%`}/>
        }
        if (task?.state === 'done') return <Button variant="secondary" disabled onClicked={() => undefined} text="Downloaded"/>
        if (folder?.free_bytes != null && f.size_bytes + DISK_MARGIN_BYTES > folder.free_bytes) {
            return <Button variant="secondary" disabled onClicked={() => undefined} text={`${size}: not enough space`}/>
        }
        return <Button variant="secondary" text={`Download ${size}`} onClicked={() => void download(repo, f.path)}/>
    }

    const taskRow = (t: HfTask) => (
        <Div key={t.id} className="models__preset">
            <Div className="models__profile-main">
                <Label className="models__profile-name" text={t.local_path}/>
                <Label variant="secondary" className={t.state === 'failed' ? 'models__error' : 'models__meta'}
                       text={t.state === 'failed' ? t.error ?? 'failed' : t.state === 'done' ? doneHint
                           : `${t.phase} · ${formatBytes(t.completed_bytes)} / ${formatBytes(t.total_bytes)}`}/>
                {t.state === 'running' ? (
                    <Div className="model-picker__bar">
                        <Div className="model-picker__bar-fill"
                             style={{'--fraction': t.total_bytes > 0 ? t.completed_bytes / t.total_bytes : 0} as CSSProperties}/>
                    </Div>
                ) : null}
            </Div>
        </Div>
    )

    const shown = (repos ?? []).filter((r) => showGated || !r.gated)
    const hidden = (repos ?? []).length - shown.length

    return (
        <Div className="models__section">
            {/* Pinned to the top of the scrolling area, so the downloads in progress stay in view while
                files further down are picked, and whatever is searched meanwhile */}
            <Div className="hf-panel__pinned">
                <Input className="model-picker__search" text={query} onChanged={setQuery}
                       placeholder="Search Hugging Face for GGUF models"/>
                {tasks.filter((t) => t.state === 'running').map((t) => taskRow(t))}
            </Div>
            {folder?.path ? (
                <Div className="hf-panel__disk">
                    <Div className="hf-panel__disk-line">
                        <Label variant="secondary" className="models__meta" text={`Saved to ${folder.path}, a folder per repository`}/>
                        {folder.free_bytes != null ? (
                            <Label variant="secondary" className="models__meta"
                                   text={folder.total_bytes ? `${formatBytes(folder.free_bytes)} free of ${formatBytes(folder.total_bytes)}` : `${formatBytes(folder.free_bytes)} free`}/>
                        ) : null}
                    </Div>
                    {folder.free_bytes != null && folder.total_bytes ? (
                        <Div className="hf-panel__disk-bar">
                            <Div className="hf-panel__disk-used" style={{width: `${(1 - folder.free_bytes / folder.total_bytes) * 100}%`}}/>
                        </Div>
                    ) : null}
                </Div>
            ) : null}
            <Div className="hf-panel__sorts">
                {SORTS.map((option) => (
                    <Button key={option.sort} variant="secondary"
                            className={`hf-panel__sort${option.sort === sort ? ' hf-panel__sort--on' : ''}`}
                            text={option.label} onClicked={() => setSort(option.sort)}/>
                ))}
            </Div>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}

            {tasks.filter((t) => t.state !== 'running').map((t) => taskRow(t))}

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
                                   text={[`${repo.downloads.toLocaleString()} downloads`, `${repo.likes.toLocaleString()} likes`,
                                       repo.created_at ? `added ${shortDate(repo.created_at)}` : null].filter(Boolean).join(' · ')}/>
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
                                {f.projector ? <Label variant="secondary" className="models__meta" text="vision projector"/> : null}
                            </Div>
                            {downloadButton(repo.id, f)}
                        </Div>
                    )) : null}
                </Div>
            ))}
            {/* Seen by the page loader when it scrolls into view */}
            <Div ref={endMarker} className="hf-panel__end">
                {loadingMore ? <Label variant="secondary" className="models__meta" text="Loading more…"/> : null}
            </Div>
        </Div>
    )
};
