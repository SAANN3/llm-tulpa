import {useState} from 'react'
import {useNavigate, useSearchParams} from 'react-router-dom'

import {Gear} from 'pixelarticons/react'

import '../styles/plugins.scss'
import type {PluginInfo} from '../api/plugins/types'
import {PluginSettings} from '../components/plugin-settings.tsx'
import {Button, Div, Label, ToggleSwitch} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {usePlugins} from '../hooks/use-plugins.ts'

const pluginKey = (plugin: PluginInfo): string => `${plugin.plugin_name}/${plugin.plugin_subname}`;

const Plugins = () => {
    useDocumentTitle('Plugins')
    const navigate = useNavigate()
    const [searchParams] = useSearchParams()
    const {plugins, loading, setEnabled, setSettings, getSchema, getHelp} = usePlugins()
    const [unconfiguredNotice, setUnconfiguredNotice] = useState<string | null>(null)

    const onBack = () => navigate('/')

    const onToggle = (plugin: PluginInfo, enabled: boolean) => {
        if (enabled && plugin.settings == null) {
            setUnconfiguredNotice(pluginKey(plugin))
            return
        }

        setUnconfiguredNotice(null)
        setEnabled(plugin.plugin_name, plugin.plugin_subname, enabled)
    }

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
            <Div className="dos-frame plugins__panel">
                <span className="dos-frame__title">Plugins</span>
                <Div className="dos-frame__body plugins__body">
                    {openPluginName != null && openPluginSubname != null ? (
                        openPlugin ? (
                            <PluginSettings
                                key={pluginKey(openPlugin)}
                                plugin={openPlugin}
                                getSchema={getSchema}
                                getHelp={getHelp}
                                onSave={(settings) => setSettings(openPlugin.plugin_name, openPlugin.plugin_subname, settings)}
                                onBack={() => navigate('/plugins')}
                            />
                        ) : loading ? (
                            <Label variant="secondary" text="Loading…"/>
                        ) : (
                            <Div className="vbox plugins__missing">
                                <Label variant="secondary" text="No such plugin."/>
                                <Button variant="secondary" text="Back" onClicked={() => navigate('/plugins')}/>
                            </Div>
                        )
                    ) : (
                        <>
                            {loading ? (
                                <Label variant="secondary" text="Loading…"/>
                            ) : plugins.length === 0 ? (
                                <Label variant="secondary" text="No plugins registered."/>
                            ) : (
                                <Div className="vbox plugins__list">
                                    {plugins.map((plugin) => (
                                        <Div key={pluginKey(plugin)} className="vbox plugins__item">
                                            <Div className="list-row plugins__row">
                                                <Div className="vbox plugins__row-main">
                                                    <Label className="plugins__name" text={plugin.plugin_subname}/>
                                                    <Label className="mono plugins__subname" variant="secondary"
                                                           text={plugin.plugin_name}/>
                                                </Div>
                                                <Div className="plugins__controls">
                                                    <ToggleSwitch toggled={plugin.enabled}
                                                                  onToggled={(enabled) => onToggle(plugin, enabled)}/>
                                                    <Div onClick={() => openSettings(plugin)} className="plugins__gear">
                                                        <Gear width={16} height={16}/>
                                                    </Div>
                                                </Div>
                                            </Div>
                                            {unconfiguredNotice === pluginKey(plugin) ? (
                                                <Label className="plugins__notice"
                                                       text="Set up this plugin's settings before enabling it."/>
                                            ) : null}
                                        </Div>
                                    ))}
                                </Div>
                            )}
                            <Button variant="secondary" text="Back" onClicked={onBack}/>
                        </>
                    )}
                </Div>
            </Div>
        </Div>
    )
};

export default Plugins
