import {useEffect, useRef, useState} from 'react'
import '../../styles/install-plugin-popup.scss'
import {getStagedPluginFile} from '../../api/plugins/external-staged-file.ts'
import type {InstallTask, StagedPlugin} from '../../api/plugins/types.ts'
import {errorReason} from '../../utils/error-reason.ts'
import {formatBytes} from '../../utils/format-bytes.ts'
import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface InstallPluginPopupProps {
    /** The plugin that was unpacked and is waiting for a yes; the popup is open while there is one */
    staged: StagedPlugin | null
    /** The install, once the owner has said yes */
    task: InstallTask | null
    error: string | null
    onInstall: () => void
    /** Leaving: throws the staged plugin away, or closes the report of an install that ended */
    onDismiss: () => void
}

const TABS = ['overview', 'files', 'readme'] as const
type Tab = (typeof TABS)[number]

const sourceText = (staged: StagedPlugin): string => {
    const {source} = staged
    if (source.source_type === 'git') return `git ${source.url ?? ''}${source.git_ref ? ` (${source.git_ref})` : ''}${source.commit ? `, commit ${source.commit.slice(0, 10)}` : ''}`
    return `a file${source.sha256 ? `, sha256 ${source.sha256.slice(0, 16)}…` : ''}`
}

/** What the plugin asks for, in words */
const asksFor = (staged: StagedPlugin): string => {
    const parts: string[] = []
    if (staged.settings.length > 0) parts.push(`settings (${staged.settings.join(', ')})`)
    if (staged.has_tab) parts.push('a tab in Settings')
    if (staged.has_database) parts.push('a database of its own')
    return parts.length > 0 ? parts.join(', ') : 'nothing special'
}

const Fact = ({name, value, code}: { name: string; value: string; code?: boolean }) => (
    <>
        <Label variant="secondary" text={name}/>
        {code ? <code className="install-plugin__code">{value}</code> : <Label text={value}/>}
    </>
)

/** The files of a staged plugin, and the text of the one the owner opens */
const Files = ({staged}: { staged: StagedPlugin }) => {
    const [open, setOpen] = useState<{ path: string; text: string | null; error: string | null } | null>(null)
    const view = async (path: string) => {
        setOpen({path, text: null, error: null})
        try {
            setOpen({path, text: await getStagedPluginFile(staged.id, path), error: null})
        } catch (e) {
            setOpen({path, text: null, error: errorReason(e, 'Could not read that file.')})
        }
    }

    if (open) {
        return (
            <Div className="install-plugin__file">
                <Div className="install-plugin__file-head">
                    <Button variant="secondary" text="Back to the list" onClicked={() => setOpen(null)}/>
                    <Label variant="secondary" text={open.path}/>
                </Div>
                {open.error ? <Label className="install-plugin__problem" text={open.error}/> : <pre className="install-plugin__pre">{open.text ?? 'Loading…'}</pre>}
            </Div>
        )
    }
    return (
        <Div className="install-plugin__files">
            <Label variant="secondary" text={`${staged.files.length}${staged.files_truncated ? '+' : ''} files, ${formatBytes(staged.total_bytes)}. Open one to read it.`}/>
            <Div className="install-plugin__file-list">
                {staged.files.map((file) => (
                    <Div key={file.path} className="install-plugin__file-row" onClick={() => void view(file.path)}>
                        <Label text={file.path}/>
                        <Label variant="secondary" text={formatBytes(file.size)}/>
                    </Div>
                ))}
            </Div>
        </Div>
    )
}

/** Everything the owner needs to decide, and the Install button */
const Review = ({staged, error, onInstall, onDismiss}: Pick<InstallPluginPopupProps, 'error' | 'onInstall' | 'onDismiss'> & { staged: StagedPlugin }) => {
    const [tab, setTab] = useState<Tab>('overview')
    const tabs = TABS.filter((id) => id !== 'readme' || staged.readme != null)
    const blocked = staged.problems.length > 0
    const verb = staged.action === 'update' ? 'Update' : 'Install'
    return (
        <Popup open onClose={onDismiss} title={`${verb} ${staged.name}/${staged.subname}`} width={760}
               tabs={tabs.map((id) => ({id, label: id === 'files' ? `files (${staged.files.length}${staged.files_truncated ? '+' : ''})` : id}))}
               activeTab={tab} onTab={(id) => setTab(id as Tab)}
               actions={blocked ? [] : [{keys: ['Enter'], shown: 'enter', label: verb.toLowerCase(), run: onInstall}]}>
            <Div className="install-plugin__body">
                {tab === 'overview' ? (
                    <Div className="install-plugin__overview">
                        <Div className="install-plugin__head">
                            <Label className="install-plugin__name" text={`${staged.name}/${staged.subname}`}/>
                            <Label variant="secondary" text={`v${staged.version}`}/>
                            <Label variant="secondary" className="install-plugin__tag"
                                   text={staged.action === 'update' ? `replaces v${staged.installed_version ?? '?'}` : 'new'}/>
                        </Div>
                        <Label text={staged.summary}/>
                        {staged.help_message ? <Label variant="secondary" className="install-plugin__help" text={staged.help_message}/> : null}
                        <Div className="install-plugin__facts">
                            <Fact name="From" value={sourceText(staged)}/>
                            <Fact name="Kind" value={staged.kind === 'prebuilt' ? 'a program that comes built' : 'built here from its sources'}/>
                            <Fact name="Asks for" value={asksFor(staged)}/>
                            {staged.check ? <Fact name="Checks with" value={staged.check} code/> : null}
                            {staged.build ? <Fact name="Builds with" value={staged.build} code/> : null}
                            <Fact name="Runs" value={staged.run} code/>
                        </Div>
                        {blocked ? (
                            <Div className="install-plugin__problems">
                                {staged.problems.map((problem) => <Label key={problem} className="install-plugin__problem" text={problem}/>)}
                            </Div>
                        ) : null}
                        <Label variant="secondary" className="install-plugin__trust"
                               text="Once installed it runs as a program on this machine with your account's rights. Read its files first if you are not sure of it."/>
                    </Div>
                ) : null}
                {tab === 'files' ? <Files staged={staged}/> : null}
                {tab === 'readme' ? <pre className="install-plugin__pre install-plugin__pre--text">{staged.readme}</pre> : null}
            </Div>
            {error ? <Label className="install-plugin__problem" text={error}/> : null}
            <PopupActions confirmLabel={verb} emphasis="confirm" confirmDisabled={blocked} onConfirm={onInstall} onCancel={onDismiss}/>
        </Popup>
    )
}

/** An install under way, or just ended: what it is doing and everything it said, the build's output included */
const Progress = ({task, onDismiss}: { task: InstallTask; onDismiss: () => void }) => {
    const log = useRef<HTMLPreElement>(null)
    useEffect(() => {
        // The newest line is the one that matters while it runs
        log.current?.scrollTo({top: log.current.scrollHeight})
    }, [task.log.length])
    const running = task.state === 'running'
    return (
        <Popup open onClose={() => !running && onDismiss()} title={`Installing ${task.name}/${task.subname}`} width={760}
               actions={running ? [] : [{keys: ['Enter'], shown: 'enter', label: 'close', run: onDismiss}]}>
            <Div className="install-plugin__body">
                <Label text={task.state === 'done' ? 'Installed.' : task.state === 'failed' ? 'It was not installed.' : `${task.step}…`}/>
                {task.error ? <Label className="install-plugin__problem" text={task.error}/> : null}
                <pre ref={log} className="install-plugin__pre install-plugin__pre--log">{task.log.join('\n') || 'Starting…'}</pre>
            </Div>
            <Div className="popup__actions">
                <Button variant="secondary" text={running ? 'Working…' : 'Close'} disabled={running} onClicked={onDismiss}/>
            </Div>
        </Popup>
    )
}

/** The window for adding a plugin: first what is in it, to read and then to confirm; then how the install goes */
export const InstallPluginPopup = ({staged, task, error, onInstall, onDismiss}: InstallPluginPopupProps) => {
    if (!staged) return null
    return task ? <Progress task={task} onDismiss={onDismiss}/> : <Review staged={staged} error={error} onInstall={onInstall} onDismiss={onDismiss}/>
};
