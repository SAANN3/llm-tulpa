import type {CSSProperties} from 'react'
import '../styles/theme-preview.scss'
import {Div, Label, RadioButton} from './primitives'
import {useTheme} from '../context/use-еheme.ts'
import {themeDisplayNames} from '../themes'
import type {ThemeName} from '../themes'

/** A miniature, non-interactive rendering of the app's layout in one theme's colors */
const ThemeMiniature = ({theme}: { theme: ThemeName }) => (
    <div data-theme={theme} className="theme-picker__miniature">
        <div className="theme-picker__mini-sidebar">
            <div className="theme-picker__bar theme-picker__bar--accent"/>
            <div className="theme-picker__bar theme-picker__bar--muted"/>
            <div className="theme-picker__bar theme-picker__bar--muted"/>
        </div>
        <div className="theme-picker__mini-content">
            <div className="theme-picker__bar theme-picker__bar--user"/>
            <div className="theme-picker__bar theme-picker__bar--wide"/>
            <div className="theme-picker__bar theme-picker__bar--narrow"/>
        </div>
    </div>
);

/** Three clickable theme cards that apply a theme immediately on pick */
export const ThemePreview = () => {
    const {themeName, setThemeName, themeNames} = useTheme()

    return (
        <div className="theme-picker" style={{'--theme-count': themeNames.length} as CSSProperties}>
            {themeNames.map((theme) => (
                <Div key={theme} className="theme-picker__card" onClick={() => setThemeName(theme)}>
                    <ThemeMiniature theme={theme}/>
                    <Div className="theme-picker__label-row">
                        <RadioButton name="theme-picker" value={theme} checked={themeName === theme}
                                     onChanged={() => setThemeName(theme)}/>
                        <Label className="theme-picker__name" text={themeDisplayNames[theme]}/>
                    </Div>
                </Div>
            ))}
        </div>
    )
};
