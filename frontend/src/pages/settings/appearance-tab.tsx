import {useState} from 'react'
import '../../styles/appearance.scss'
import {backgroundEffects, findBackground, type BackgroundId} from '../../backgrounds'
import {AppearancePreview} from '../../components/appearance-preview.tsx'
import {Div, Input, Label, Slider, ToggleSwitch} from '../../components/primitives'
import {TimeFormatChoice} from '../../components/settings-fields.tsx'
import {useTheme} from '../../context/use-theme.ts'
import {themeDisplayNames, themeGroups, type ThemeName} from '../../themes'

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
 * once and is kept in this browser, not saved with the account's settings. */
export const AppearanceTab = () => {
    const {themeName, setThemeName, background, setBackground, timeFormat, sidebarSeeThrough, setSidebarSeeThrough} = useTheme()
    const [themeFilter, setThemeFilter] = useState('')
    const [backgroundFilter, setBackgroundFilter] = useState('')
    const [pointedTheme, setPointedTheme] = useState<ThemeName | null>(null)
    const [pointedBackground, setPointedBackground] = useState<BackgroundId | null>(null)

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
        <Div className="appearance">
            <Div className="appearance__list" onHover={(on) => !on && setPointedTheme(null)}>
                <Input className="appearance__filter" text={themeFilter} onChanged={setThemeFilter} placeholder="Filter themes"/>
                <Div className="appearance__items">
                    {themeRows.map((group) => (
                        <Div key={group.name}>
                            <Label variant="secondary" className="appearance__group" text={group.name}/>
                            {group.themes.map((theme) => (
                                <Div key={theme}
                                     className={`appearance__item${theme === themeName ? ' appearance__item--on' : ''}${theme === pointedTheme ? ' appearance__item--pointed' : ''}`}
                                     onHover={(on) => on && setPointedTheme(theme)}
                                     onClick={() => setThemeName(theme)}>
                                    <Swatches theme={theme}/>
                                    <span>{themeDisplayNames[theme]}</span>
                                </Div>
                            ))}
                        </Div>
                    ))}
                    {themeRows.length === 0 ? <Label variant="secondary" className="appearance__empty" text="No theme matches."/> : null}
                </Div>
            </Div>

            <Div className="appearance__list" onHover={(on) => !on && setPointedBackground(null)}>
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

            <Div className="appearance__side">
                <AppearancePreview theme={shownTheme} background={shownBackground} strength={background.strength} speed={background.speed}
                                   sidebarSeeThrough={sidebarSeeThrough} timeFormat={timeFormat}
                                   note={`${themeDisplayNames[shownTheme]} · ${backgroundName}${previewing ? ' · click to use' : ''}`}/>
                <Div className="appearance__look">
                    <Label text="Brightness"/>
                    <Slider value={background.strength} onChanged={(strength) => setBackground({strength})} min={0.3} max={2} step={0.05} disabled={!animated}/>
                    <Label text="Speed"/>
                    <Slider value={background.speed} onChanged={(speed) => setBackground({speed})} min={0.3} max={2} step={0.05} disabled={!animated}/>
                    <Label text="See-through sidebar"/>
                    <Div className="appearance__inline">
                        <ToggleSwitch toggled={sidebarSeeThrough} onToggled={setSidebarSeeThrough}/>
                        <Label variant="secondary" text="the background shows faintly behind it"/>
                    </Div>
                    <Label text="Time format"/>
                    <TimeFormatChoice/>
                </Div>
            </Div>
        </Div>
    )
};
