import {useState} from 'react'
import {useNavigate, useParams, useSearchParams} from 'react-router-dom'

import {Gear} from 'pixelarticons/react'

import '../styles/plugins.scss'
import {Frame} from '../components/frame.tsx'
import type {ExternalPlugin, PluginInfo} from '../api/plugins/types'
import {ExternalPluginDetails} from '../components/external-plugin-details.tsx'
import {ExternalPluginMenu} from '../components/external-plugin-menu.tsx'
import {InstallPluginPopup} from '../components/popups/install-plugin-popup.tsx'
import {PluginInstall} from '../components/plugin-install.tsx'
import {PluginSettings} from '../components/plugin-settings.tsx'
import {Button, Div, Label, ToggleSwitch} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useExternalPlugins} from '../hooks/use-external-plugins.ts'
import {usePluginInstall} from '../hooks/use-plugin-install.ts'
import {usePlugins} from '../hooks/use-plugins.ts'
import {useGoBack} from '../hooks/use-go-back.ts'
import {errorReason} from '../utils/error-reason.ts'

const pluginKey = (plugin: PluginInfo): string => `${plugin.plugin_name}/${plugin.plugin_subname}`;
const externalKey = (plugin: ExternalPlugin): string => `${plugin.name}/${plugin.subname}`;

const TABS = ['Plugins', 'Install'] as const

/** A name as a title: a short one (an abbreviation, `vk`) in capitals, a longer one capitalized */
const titled = (name: string): string => (name.length <= 2 ? name.toUpperCase() : name[0].toUpperCase() + name.slice(1))

/** The plugins by kind, in the order the backend lists them (by kind, then name) */
const byKind = (plugins: PluginInfo[]): [string, PluginInfo[]][] => {
    const kinds = new Map<string, PluginInfo[]>()
    plugins.forEach((p) => kinds.set(p.plugin_name, [...(kinds.get(p.plugin_name) ?? []), p]))
    return [...kinds]
}

const Plugins = () => {
    useDocumentTitle('Plugins')
    const navigate = useNavigate()
    const [searchParams] = useSearchParams()
    const {tab: tabSlug} = useParams()
    const tab = TABS.find((name) => name.toLowerCase() === tabSlug) ?? TABS[0]
    const {plugins, loading, reload: reloadPlugins, setEnabled, setSettings, getSchema, getHelp} = usePlugins()
    const externals = useExternalPlugins()
    const install = usePluginInstall(() => {
        void reloadPlugins()
        void externals.reload()
    })
    const [notice, setNotice] = useState<{key: string; message: string} | null>(null)
    // The external plugins whose details are open
    const [opened, setOpened] = useState<string[]>([])
    const externalByKey = new Map(externals.external.map((p) => [externalKey(p), p]))
    const registered = new Set(plugins.map(pluginKey))
    // External plugins that are installed but not in the list above: they could not start, or were unloaded
    const notRunning = externals.external.filter((p) => !registered.has(externalKey(p)))

    const onBack = useGoBack('/')
    // A plugin's settings were opened from the list, so stepping back returns to it
    const backToList = useGoBack('/plugins')

    const onToggle = async (plugin: PluginInfo, enabled: boolean) => {
        const key = pluginKey(plugin)
        if (enabled && plugin.settings == null) {
            setNotice({key, message: "Set up this plugin's settings before enabling it."})
            return
        }

        setNotice(null)
        try {
            await setEnabled(plugin.plugin_name, plugin.plugin_subname, enabled)
            // An external plugin starts or ends its process with the switch, which the details show
            if (externalByKey.has(key)) void externals.reload()
        } catch (e) {
            setNotice({key, message: errorReason(e, 'Something went wrong changing this plugin — try again.')})
        }
    }

    const toggleDetails = (key: string) => setOpened((prev) => (prev.includes(key) ? prev.filter((k) => k !== key) : [...prev, key]))

    /** What the owner does with an external plugin: the page reads both lists again after, and says why when it did not work */
    const manage = async (plugin: ExternalPlugin, what: () => Promise<void>) => {
        setNotice(null)
        try {
            await what()
            await reloadPlugins()
        } catch (e) {
            setNotice({key: externalKey(plugin), message: errorReason(e, 'That did not work.')})
        }
    }

    const menuFor = (plugin: ExternalPlugin, loaded: boolean) => (
        <ExternalPluginMenu plugin={plugin} loaded={loaded}
                            onLoad={() => void manage(plugin, () => externals.load(plugin))}
                            onUnload={() => void manage(plugin, () => externals.unload(plugin))}
                            onExport={() => void manage(plugin, () => externals.exportFile(plugin))}
                            onUninstall={(removeData) => void manage(plugin, () => externals.uninstall(plugin, removeData))}/>
    )

    const openSettings = (plugin: PluginInfo) =>
        navigate(`/plugins?plugin_name=${encodeURIComponent(plugin.plugin_name)}&plugin_subname=${encodeURIComponent(plugin.plugin_subname)}`)

    const openPluginName = searchParams.get('plugin_name')
    const openPluginSubname = searchParams.get('plugin_subname')
    const openPlugin =
        openPluginName != null && openPluginSubname != null
            ? plugins.find((p) => p.plugin_name === openPluginName && p.plugin_subname === openPluginSubname)
            : null

    return (
        <Div className="page center vbox plugins">
            <TypewriterLabel className="plugins__title" text="[ Plugins ]" charIntervalMs={30}/>
            <Frame className="plugins__panel" bodyClassName="plugins__body" title="Plugins"
                   // A plugin's settings are a view of their own, not a tab
                   tabs={openPluginName == null ? TABS.map((name) => ({id: name, label: name})) : undefined} activeTab={tab}
                   onTab={(id) => navigate(`/plugins/${id.toLowerCase()}`, {replace: true})}
                   actions={[]} onEscape={openPluginName != null ? backToList : onBack} escapeLabel="back">
                {openPluginName != null && openPluginSubname != null ? (
                    openPlugin ? (
                        <PluginSettings
                            key={pluginKey(openPlugin)}
                            plugin={openPlugin}
                            getSchema={getSchema}
                            getHelp={getHelp}
                            onSave={(settings) => setSettings(openPlugin.plugin_name, openPlugin.plugin_subname, settings)}
                            onBack={backToList}
                        />
                    ) : loading ? (
                        <Label variant="secondary" text="Loading…"/>
                    ) : (
                        <Div className="vbox plugins__missing">
                            <Label variant="secondary" text="No such plugin."/>
                            <Button variant="secondary" text="Back" onClicked={backToList}/>
                        </Div>
                    )
                ) : tab === 'Install' ? (
                    <>
                        <PluginInstall install={install}/>
                        <Button variant="secondary" text="Back" onClicked={onBack}/>
                    </>
                ) : (
                    <>
                        {loading ? (
                            <Label variant="secondary" text="Loading…"/>
                        ) : plugins.length === 0 ? (
                            <Label variant="secondary" text="No plugins registered."/>
                        ) : (
                            <Div className="vbox plugins__list">
                                {byKind(plugins).map(([kind, list]) => (
                                    <Div key={kind} className="vbox plugins__kind">
                                        <Label variant="secondary" className="plugins__kind-name" text={kind}/>
                                        {list.map((plugin) => {
                                            const external = externalByKey.get(pluginKey(plugin))
                                            return (
                                            <Div key={pluginKey(plugin)} className="vbox plugins__item">
                                                <Div className="plugins__row">
                                                    <Div className="vbox plugins__row-main">
                                                        <Label className="plugins__name" text={titled(plugin.plugin_subname)}/>
                                                        <Label className="plugins__summary" variant="secondary" text={plugin.summary}/>
                                                    </Div>
                                                    <Div className="plugins__controls">
                                                        <ToggleSwitch toggled={plugin.enabled}
                                                                      onToggled={(enabled) => onToggle(plugin, enabled)}/>
                                                        <Button variant="secondary" className="plugins__gear" title={`${titled(plugin.plugin_subname)} settings`}
                                                                onClicked={() => openSettings(plugin)}>
                                                            <Gear width={18} height={18}/>
                                                        </Button>
                                                        {external ? menuFor(external, true) : null}
                                                    </Div>
                                                </Div>
                                                {external ? (
                                                    <ExternalPluginDetails plugin={external} expanded={opened.includes(pluginKey(plugin))}
                                                                           onToggle={() => toggleDetails(pluginKey(plugin))}/>
                                                ) : null}
                                                {notice?.key === pluginKey(plugin) ? (
                                                    <Label className="plugins__notice" text={notice.message}/>
                                                ) : null}
                                            </Div>
                                            )
                                        })}
                                    </Div>
                                ))}
                                {notRunning.length > 0 ? (
                                    <Div className="vbox plugins__kind plugins__not-running">
                                        <Label variant="secondary" className="plugins__kind-name" text="installed, not running"/>
                                        {notRunning.map((plugin) => (
                                            <Div key={externalKey(plugin)} className="vbox plugins__item">
                                                <Div className="plugins__row">
                                                    <Div className="vbox plugins__row-main">
                                                        <Label className="plugins__name" text={`${plugin.name}/${plugin.subname}`}/>
                                                    </Div>
                                                    <Div className="plugins__controls">{menuFor(plugin, false)}</Div>
                                                </Div>
                                                <ExternalPluginDetails plugin={plugin} expanded={opened.includes(externalKey(plugin))}
                                                                       onToggle={() => toggleDetails(externalKey(plugin))}/>
                                                {notice?.key === externalKey(plugin) ? <Label className="plugins__notice" text={notice.message}/> : null}
                                            </Div>
                                        ))}
                                    </Div>
                                ) : null}
                            </Div>
                        )}
                        <Button variant="secondary" text="Back" onClicked={onBack}/>
                    </>
                )}
            </Frame>
            <InstallPluginPopup staged={install.staged} task={install.task} error={install.error} onInstall={() => void install.install()}
                                onDismiss={() => void install.dismiss()}/>
        </Div>
    )
};

export default Plugins
