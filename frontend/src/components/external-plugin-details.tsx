import '../styles/external-plugin-details.scss'
import type {ExternalPlugin} from '../api/plugins/types.ts'
import {Div, Label} from './primitives'

export interface ExternalPluginDetailsProps {
    plugin: ExternalPlugin
    expanded: boolean
    onToggle: () => void
}

/** What an installed plugin came from, in a few words */
const sourceText = (plugin: ExternalPlugin): string => {
    switch (plugin.source_type) {
        case 'git':
            return `git ${plugin.source_url ?? ''}${plugin.source_ref ? ` @ ${plugin.source_ref.slice(0, 10)}` : ''}`
        case 'file':
            return `a file${plugin.source_sha256 ? ` (sha256 ${plugin.source_sha256.slice(0, 12)}…)` : ''}`
        default:
            return plugin.source_url ? `a folder on this machine (${plugin.source_url})` : 'put in the plugins folder by hand'
    }
}

const stateText = (plugin: ExternalPlugin): string => {
    if (plugin.state === 'unavailable') return 'could not start'
    if (plugin.state === 'failing') return 'crashed too often, switched off'
    if (plugin.state === 'unloaded') return 'unloaded'
    switch (plugin.process?.state) {
        case 'running':
            return 'running'
        case 'restarting':
            return 'restarting after a crash'
        case 'failed':
            return 'crashed too often'
        default:
            // Loaded but switched off: a plugin that is off has no process
            return 'off'
    }
}

/** A line under an external plugin's name: that it is one, its version, where it came from and how it is doing, with the
 * details (its process, its last error, the end of its log) one click away */
export const ExternalPluginDetails = ({plugin, expanded, onToggle}: ExternalPluginDetailsProps) => {
    const problem = plugin.state !== 'ok' || plugin.process?.state === 'restarting' || plugin.process?.state === 'failed'
    const error = plugin.last_error ?? plugin.process?.last_error ?? null
    const logs = plugin.process?.logs ?? []
    return (
        <Div className="external-plugin">
            <Div className="external-plugin__line" onClick={onToggle}>
                <Label variant="secondary" className="external-plugin__badge" text="external"/>
                <Label variant="secondary" className="external-plugin__meta" text={`${plugin.version ? `v${plugin.version} · ` : ''}${stateText(plugin)}`}/>
                <Label variant="secondary" className="external-plugin__more" text={expanded ? '[ − ]' : '[ + ]'}/>
            </Div>
            {error && !expanded ? <Label className="external-plugin__error external-plugin__error--short" text={error}/> : null}
            {expanded ? (
                <Div className="external-plugin__details">
                    <Div className="external-plugin__facts">
                        <Label variant="secondary" text="From"/><Label text={sourceText(plugin)}/>
                        <Label variant="secondary" text="Installed"/><Label text={new Date(plugin.installed_at).toLocaleString()}/>
                        {plugin.updated_at !== plugin.installed_at
                            ? <><Label variant="secondary" text="Updated"/><Label text={new Date(plugin.updated_at).toLocaleString()}/></> : null}
                        {plugin.process ? (
                            <>
                                <Label variant="secondary" text="Process"/>
                                <Label text={`${plugin.process.state}${plugin.process.pid ? ` (pid ${plugin.process.pid})` : ''}${plugin.process.restarts ? `, restarted ${plugin.process.restarts} time${plugin.process.restarts === 1 ? '' : 's'}` : ''}`}/>
                            </>
                        ) : null}
                    </Div>
                    {problem && error ? <Label className="external-plugin__error" text={error}/> : null}
                    {logs.length > 0 ? (
                        <>
                            <Label variant="secondary" className="external-plugin__log-title" text="Log"/>
                            <pre className="external-plugin__log">{logs.join('\n')}</pre>
                        </>
                    ) : null}
                </Div>
            ) : null}
        </Div>
    )
};
