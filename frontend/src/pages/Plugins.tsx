import { useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'

import type { PluginInfo } from '../api/plugins/types'
import { GearIcon } from '../components/GearIcon'
import { PluginSettings } from '../components/PluginSettings'
import { Button, Div, Label, ToggleSwitch } from '../components/primitives'
import { TypewriterLabel } from '../components/TypewriterLabel'
import { useDocumentTitle } from '../hooks/useDocumentTitle'
import { usePlugins } from '../hooks/usePlugins'

function pluginKey(plugin: PluginInfo): string {
  return `${plugin.plugin_name}/${plugin.plugin_subname}`
}

function Plugins() {
  useDocumentTitle('Plugins')
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const { plugins, loading, setEnabled, setSettings, getSchema, getHelp } = usePlugins()
  // Which row (if any) is showing the "set settings first" notice — keyed rather than a
  // plain flag so switching between plugins doesn't leave a stale notice under the wrong
  // row.
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

  // `plugin_name`/`plugin_subname` in the query pick which view this page shows —
  // the settings form for that one plugin instead of the list — without a separate
  // route. Looked up in the already-fetched `plugins` list rather than a fresh fetch,
  // per-plugin settings aren't worth their own endpoint when the list already has them.
  const openPluginName = searchParams.get('plugin_name')
  const openPluginSubname = searchParams.get('plugin_subname')
  const openPlugin =
    openPluginName != null && openPluginSubname != null
      ? plugins.find((p) => p.plugin_name === openPluginName && p.plugin_subname === openPluginSubname)
      : null

  return (
    <Div className="page center vbox" style={{ gap: 16 }}>
      <TypewriterLabel className="mono" text="[ Plugins ]" charIntervalMs={30} style={{ fontSize: 15, letterSpacing: '0.14em' }} />
      <Div
        className="vbox"
        style={{
          width: 440,
          padding: 26,
          gap: 20,
          borderRadius: 14,
          border: '1px solid var(--color-border)',
          background: 'var(--color-surface)',
        }}
      >
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
            <Label variant="secondary" text="Loading…" />
          ) : (
            <Div className="vbox" style={{ gap: 16 }}>
              <Label variant="secondary" text="No such plugin." />
              <Button variant="secondary" text="Back" onClicked={() => navigate('/plugins')} />
            </Div>
          )
        ) : (
          <>
            {loading ? (
              <Label variant="secondary" text="Loading…" />
            ) : plugins.length === 0 ? (
              <Label variant="secondary" text="No plugins registered." />
            ) : (
              <Div className="vbox" style={{ gap: 2, maxHeight: '50vh', overflowY: 'auto' }}>
                {plugins.map((plugin) => (
                  <Div key={pluginKey(plugin)} className="vbox" style={{ gap: 4 }}>
                    <Div
                      className="list-row"
                      style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', cursor: 'default' }}
                    >
                      <Div className="vbox" style={{ gap: 2 }}>
                        <Label text={plugin.plugin_subname} style={{ fontSize: 15 }} />
                        <Label className="mono" variant="secondary" text={plugin.plugin_name} style={{ fontSize: 11, opacity: 0.6 }} />
                      </Div>
                      <Div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                        <ToggleSwitch toggled={plugin.enabled} onToggled={(enabled) => onToggle(plugin, enabled)} />
                        <Div onClick={() => openSettings(plugin)} style={{ cursor: 'pointer', display: 'flex', opacity: 0.7 }}>
                          <GearIcon size={16} />
                        </Div>
                      </Div>
                    </Div>
                    {unconfiguredNotice === pluginKey(plugin) ? (
                      <Label
                        text="Set up this plugin's settings before enabling it."
                        style={{ fontSize: 12.5, padding: '0 8px', color: 'var(--color-tertiary)' }}
                      />
                    ) : null}
                  </Div>
                ))}
              </Div>
            )}
            <Button variant="secondary" text="Back" onClicked={onBack} />
          </>
        )}
      </Div>
    </Div>
  )
}

export default Plugins
