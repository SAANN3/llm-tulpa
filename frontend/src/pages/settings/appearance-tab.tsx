import {useEffect, useRef, useState} from 'react'
import '../../styles/appearance.scss'
import {backgroundEffects, findBackground, type BackgroundId} from '../../backgrounds'
import {AppearancePreview} from '../../components/appearance-preview.tsx'
import {Button, Div, Input, Label, Slider, ToggleSwitch} from '../../components/primitives'
import {TimeFormatChoice} from '../../components/settings-fields.tsx'
import {useSettings} from '../../context/use-settings.ts'
import {useTheme} from '../../context/use-theme.ts'
import {themeDisplayNames, themeGroups, type ThemeName} from '../../themes'
import {homeLines, type HomeLines} from '../../api/prompts/home-lines.ts'

const linesText = (saved: string[] | undefined, defaults: string[]): string =>
    (saved && saved.length > 0 ? saved : defaults).join('\n')

/** Blank lines are dropped. A list identical to `defaults` is stored empty, so the built-in lines stay in use. */
const linesToSave = (text: string, defaults: string[]): string[] => {
    const lines = text.split('\n').map((line) => line.trim()).filter((line) => line.length > 0)
    const same = lines.length === defaults.length && lines.every((line, index) => line === defaults[index])
    return same ? [] : lines
}

/** One line per greeting or placeholder. Blur saves the list; the same text as the built-in lines stores nothing. */
const LineList = ({text, onChanged, onCommit, onDefaults}: {
    text: string
    onChanged: (text: string) => void
    onCommit: () => void
    onDefaults: () => void
}) => (
    <Div className="appearance__lines">
        <textarea data-variant="secondary" rows={8} value={text} onChange={(event) => onChanged(event.target.value)} onBlur={onCommit}/>
        <Button variant="secondary" text="Use defaults" onClicked={onDefaults}/>
    </Div>
);

/** The two plain choices, listed before the animated backgrounds */
const PLAIN: { id: BackgroundId; name: string; group: string }[] = [{id: 'off', name: 'Off', group: 'Plain'}, {id: 'dots', name: 'Dots', group: 'Plain'}]
const BACKGROUNDS = [...PLAIN, ...backgroundEffects.map(({id, name, group}) => ({id, name, group}))]

const matches = (text: string, filter: string) => text.toLowerCase().includes(filter.trim().toLowerCase())

/** Groups `items` by their group, keeping the list's order */
const grouped = <T extends { group: string }>(items: T[]): [string, T[]][] => {
    const groups = new Map<string, T[]>()
    items.forEach((item) => groups.set(item.group, [...(groups.get(item.group) ?? []), item]))
    return [...groups]
}

/** The theme's three colors as small squares; each takes its color from its own `data-theme`, so no color is
 * written here */
const Swatches = ({theme}: { theme: ThemeName }) => (
    <span data-theme={theme} className="appearance__swatches">
        <i className="appearance__swatch appearance__swatch--secondary"/>
        <i className="appearance__swatch appearance__swatch--primary"/>
        <i className="appearance__swatch appearance__swatch--tertiary"/>
    </span>
);

/** The theme, the background and how the pages look, in one view: two lists to pick from and a preview of the
 * page. Pointing at an entry shows it in the preview; clicking applies it (a theme with its reveal). It all applies at
 * once and is kept in this browser, not saved with the account's settings. Without `controls` it is only the two
 * lists and the preview (the first-run wizard, which asks the time format on a step of its own). */
export const AppearanceTab = ({controls = true}: { controls?: boolean }) => {
    const {themeName, setThemeName, background, setBackground, timeFormat, sidebarSeeThrough, setSidebarSeeThrough, streamReplies, setStreamReplies} = useTheme()
    const {settings, setSettings} = useSettings()
    const [greetingText, setGreetingText] = useState('')
    const [placeholderText, setPlaceholderText] = useState('')
    const [defaults, setDefaults] = useState<HomeLines | null>(null)
    const textsLoaded = useRef(false)
    useEffect(() => {
        if (!controls) return
        let cancelled = false
        homeLines().then((lines) => !cancelled && setDefaults(lines)).catch(() => {})
        return () => {
            cancelled = true
        }
    }, [controls])
    useEffect(() => {
        if (!settings || !defaults || textsLoaded.current) return
        textsLoaded.current = true
        setGreetingText(linesText(settings.static_greeting, defaults.greetings))
        setPlaceholderText(linesText(settings.static_placeholder, defaults.placeholders))
    }, [settings, defaults])
    const [themeFilter, setThemeFilter] = useState('')
    const [backgroundFilter, setBackgroundFilter] = useState('')
    const [pointedTheme, setPointedTheme] = useState<ThemeName | null>(null)
    const [pointedBackground, setPointedBackground] = useState<BackgroundId | null>(null)
    const [mobileTab, setMobileTab] = useState<'themes' | 'backgrounds' | 'preview'>('themes')

    const shownTheme = pointedTheme ?? themeName
    const shownBackground = pointedBackground ?? background.id
    const backgroundName = BACKGROUNDS.find((b) => b.id === shownBackground)?.name ?? ''
    const previewing = pointedTheme != null || pointedBackground != null
    const animated = findBackground(background.id) != null

    const themeRows = themeGroups
        .map((group) => ({name: group.name, themes: group.themes.filter((t) => matches(`${themeDisplayNames[t]} ${group.name}`, themeFilter))}))
        .filter((group) => group.themes.length > 0)
    const backgroundRows = grouped(BACKGROUNDS.filter((b) => matches(`${b.name} ${b.group}`, backgroundFilter)))

    return (
        <Div className={`appearance appearance--tab-${mobileTab}`}>
            <Div className="appearance__mobile-nav">
                <Button
                    className="appearance__mobile-tab"
                    variant={mobileTab === 'themes' ? 'primary' : 'secondary'}
                    text="Themes"
                    onClicked={() => setMobileTab('themes')}
                />
                <Button
                    className="appearance__mobile-tab"
                    variant={mobileTab === 'backgrounds' ? 'primary' : 'secondary'}
                    text="Backgrounds"
                    onClicked={() => setMobileTab('backgrounds')}
                />
                <Button
                    className="appearance__mobile-tab"
                    variant={mobileTab === 'preview' ? 'primary' : 'secondary'}
                    text={controls ? "Options" : "Preview"}
                    onClicked={() => setMobileTab('preview')}
                />
            </Div>

            <Div className="appearance__list appearance__list--themes" onHover={(on) => !on && setPointedTheme(null)}>
                <Input className="appearance__filter" text={themeFilter} onChanged={setThemeFilter} placeholder="Filter themes"/>
                <Div className="appearance__items">
                    {themeRows.map((group) => (
                        <Div key={group.name}>
                            <Label variant="secondary" className="appearance__group" text={group.name}/>
                            {group.themes.map((theme) => (
                                <Div key={theme}
                                     className={`appearance__item${theme === themeName ? ' appearance__item--on' : ''}${theme === pointedTheme ? ' appearance__item--pointed' : ''}`}
                                     onHover={(on) => on && setPointedTheme(theme)}
                                     onClick={(e) => theme !== themeName && setThemeName(theme, e.currentTarget)}>
                                    <Swatches theme={theme}/>
                                    <span>{themeDisplayNames[theme]}</span>
                                </Div>
                            ))}
                        </Div>
                    ))}
                    {themeRows.length === 0 ? <Label variant="secondary" className="appearance__empty" text="No theme matches."/> : null}
                </Div>
            </Div>

            <Div className="appearance__list appearance__list--backgrounds" onHover={(on) => !on && setPointedBackground(null)}>
                <Input className="appearance__filter" text={backgroundFilter} onChanged={setBackgroundFilter} placeholder="Filter backgrounds"/>
                <Div className="appearance__items">
                    {backgroundRows.map(([group, items]) => (
                        <Div key={group}>
                            <Label variant="secondary" className="appearance__group" text={group}/>
                            {items.map((item) => (
                                <Div key={item.id}
                                     className={`appearance__item${item.id === background.id ? ' appearance__item--on' : ''}${item.id === pointedBackground ? ' appearance__item--pointed' : ''}`}
                                     onHover={(on) => on && setPointedBackground(item.id)}
                                     onClick={() => setBackground({id: item.id})}>
                                    <span>{item.name}</span>
                                </Div>
                            ))}
                        </Div>
                    ))}
                    {backgroundRows.length === 0 ? <Label variant="secondary" className="appearance__empty" text="No background matches."/> : null}
                </Div>
            </Div>

            <Div className="appearance__side appearance__side--preview">
                <AppearancePreview theme={shownTheme} background={shownBackground} strength={background.strength} speed={background.speed}
                                   sidebarSeeThrough={sidebarSeeThrough} timeFormat={timeFormat} streamReplies={streamReplies}
                                   note={`${themeDisplayNames[shownTheme]} · ${backgroundName}${previewing ? ' · click to use' : ''}`}/>
                {controls ? <Div className="appearance__look">
                    <Label text="Brightness"/>
                    <Slider value={background.strength} onChanged={(strength) => setBackground({strength})} min={0.3} max={2} step={0.05} disabled={!animated}/>
                    <Label text="Speed"/>
                    <Slider value={background.speed} onChanged={(speed) => setBackground({speed})} min={0.3} max={2} step={0.05} disabled={!animated}/>
                    <Label text="See-through sidebar"/>
                    <Div className="appearance__inline">
                        <ToggleSwitch toggled={sidebarSeeThrough} onToggled={setSidebarSeeThrough}/>
                        <Label variant="secondary" text="the background shows faintly behind it"/>
                    </Div>
                    <Label text="Live replies"/>
                    <Div className="appearance__inline">
                        <ToggleSwitch toggled={streamReplies} onToggled={setStreamReplies}/>
                        <Label variant="secondary" text="a reply shows as it is written"/>
                    </Div>
                    <Label text="Time format"/>
                    <TimeFormatChoice/>
                    <Label text="Greeting"/>
                    <Div className="appearance__inline">
                        <ToggleSwitch toggled={settings?.generate_greeting !== false} onToggled={(on) => void setSettings({generate_greeting: on})}/>
                        <Label variant="secondary" text="a new greeting each visit. Off, one of the lines below"/>
                    </Div>
                    {settings?.generate_greeting === false && defaults ? (
                        <LineList text={greetingText} onChanged={setGreetingText}
                                  onCommit={() => void setSettings({static_greeting: linesToSave(greetingText, defaults.greetings)})}
                                  onDefaults={() => {
                                      setGreetingText(defaults.greetings.join('\n'))
                                      void setSettings({static_greeting: []})
                                  }}/>
                    ) : null}
                    <Label text="Suggestions"/>
                    <Div className="appearance__inline">
                        <ToggleSwitch toggled={settings?.generate_placeholder !== false} onToggled={(on) => void setSettings({generate_placeholder: on})}/>
                        <Label variant="secondary" text="a new suggestion in the input. Off, one of the lines below"/>
                    </Div>
                    {settings?.generate_placeholder === false && defaults ? (
                        <LineList text={placeholderText} onChanged={setPlaceholderText}
                                  onCommit={() => void setSettings({static_placeholder: linesToSave(placeholderText, defaults.placeholders)})}
                                  onDefaults={() => {
                                      setPlaceholderText(defaults.placeholders.join('\n'))
                                      void setSettings({static_placeholder: []})
                                  }}/>
                    ) : null}
                </Div> : null}
            </Div>
        </Div>
    )
};
