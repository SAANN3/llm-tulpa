import {useEffect, useState} from 'react'
import type {ReactNode} from 'react'
import '../styles/settings-fields.scss'
import {Button, ChoiceGroup, Div, Input, Label, Select, Slider, ToggleSwitch} from './primitives'
import {useTheme} from '../context/use-theme.ts'
import {formatTime, type TimeFormat} from '../utils/time-format.ts'
import {ChooseModelPopup} from './popups/choose-model-popup.tsx'
import {useModelsData} from '../hooks/use-models-data.ts'
import {useThinkingDefaults} from '../hooks/use-thinking-defaults.ts'
import {getThinkingCapability} from '../api/llm/thinking-capability.ts'
import {offeredLevels} from '../utils/thinking-level.ts'
import {parseMaxTurnSteps} from '../utils/parse-max-turn-steps.ts'
import {validateTimezone} from '../utils/validate-timezone.ts'
import {PasswordInput} from './password-input.tsx'

export interface SettingsRowProps {
    label: string
    /** What the setting does, under its name */
    help?: string
    /** Draws the help in the accent: a warning that applies now (a risky switch is on, a value is wrong) */
    accent?: boolean
    /** The control under the text, as wide as the row, for one that needs the room (a name field) */
    full?: boolean
    /** The control at the top of the row, level with the name, instead of centered on the whole row: for a row whose help
     * text is long, so the control stays beside the name */
    top?: boolean
    /** On a phone the control goes under the text, at the left: for a control too wide to sit beside a long text (a button
     * with a long name). A short one stays at the right, like in every other row. */
    stack?: boolean
    /** Under the text and the control, across the row: what goes with the setting but is wider than a control (the progress
     * of an install, why it failed) */
    below?: ReactNode
    children: ReactNode
}

/** One setting: its name and help on the left, its control at the right edge; or, `full`, the control under them
 * across the row. Every row in Settings is one of the two, so the controls line up down the page. */
export const SettingsRow = ({label, help, accent, full, top, stack, below, children}: SettingsRowProps) => (
    <Div className={`settings-row${full ? ' settings-row--full' : ''}${top ? ' settings-row--top' : ''}${stack ? ' settings-row--stack' : ''}`}>
        <Div className="settings-row__text">
            <Label className="settings-row__label" text={label}/>
            {help ? <Label variant="secondary" className={`settings-row__help${accent ? ' settings-row__help--accent' : ''}`} text={help}/> : null}
        </Div>
        <Div className="settings-row__control">{children}</Div>
        {below ? <Div className="settings-row__below">{below}</Div> : null}
    </Div>
);

/** A titled group of rows */
export const SettingsSection = ({title, children}: { title: string; children: ReactNode }) => (
    <Div className="settings-section">
        <Label className="settings-section__title" text={title}/>
        {children}
    </Div>
);

export interface NameTimezoneRowProps {
    name: string
    onNameChanged: (name: string) => void
    timezoneText: string
    onTimezoneChanged: (text: string) => void
}

/** The name across the row and the timezone as a short UTC offset at its right end */
export const NameTimezoneRow = ({name, onNameChanged, timezoneText, onTimezoneChanged}: NameTimezoneRowProps) => {
    const tz = validateTimezone(timezoneText)
    const nameMissing = name.trim().length === 0
    return (
        <SettingsRow label="Name and timezone" full accent={nameMissing || !tz.valid}
                     help={nameMissing ? 'Enter a name.' : tz.valid ? 'The model is told your name, and the time where you are.' : tz.message}>
            <Div className="settings-row__name-tz">
                <Input className="settings-row__name" text={name} onChanged={onNameChanged} placeholder="Your name"/>
                <Label variant="secondary" text="UTC"/>
                <Input className="settings-row__tz" text={timezoneText} onChanged={onTimezoneChanged} placeholder="+3"/>
            </Div>
        </SettingsRow>
    )
};

export interface ActiveModelFieldProps {
    provider: string
    model: string | null
    /** The launch profile that was chosen with it, when one was; null means the model's first */
    launchProfileId: number | null
    onChosen: (provider: string, model: string, launchProfileId?: number) => void
}

/** The model new chats start with — an existing chat keeps the model it's bound to */
export const ActiveModelField = ({provider, model, launchProfileId, onChosen}: ActiveModelFieldProps) => {
    const [open, setOpen] = useState(false)
    const {models, profiles} = useModelsData(false)

    // New chats start on the profile that was chosen, else the model's first; Ollama's models have none
    const shownProfileId = provider === 'ollama' ? undefined : launchProfileId ?? models.find((m) => m.file === model)?.profile_ids[0]
    const profileName = profiles.find((p) => p.id === shownProfileId)?.name ?? null

    return (
        <SettingsRow label="Default model"
                     help="New chats start with this model. Each chat can be switched with the model button under its message box.">
            <Label className="settings-row__value" text={profileName ? `${model ?? 'none selected'} · ${profileName}` : model ?? 'none selected'}/>
            <Button variant="secondary" text="Change" onClicked={() => setOpen(true)}/>
            <ChooseModelPopup
                open={open}
                provider={provider}
                selected={provider === 'ollama' ? model : null}
                selectedProfileId={shownProfileId ?? null}
                onSelect={(chosen) => {
                    onChosen('ollama', chosen)
                    setOpen(false)
                }}
                onSelectProfile={(profile) => {
                    onChosen(profile.provider, profile.model, profile.id)
                    setOpen(false)
                }}
                onClose={() => setOpen(false)}
            />
        </SettingsRow>
    )
};

/** What the Select says for a model with no chosen level: it starts on its first one */
const FIRST_LEVEL = "Model's first level"

export interface ThinkingDefaultFieldProps {
    provider: string
    /** The default model; the row is left out until there is one */
    model: string | null
}

/** The thinking level new chats start on for the default model. Saved at once, apart from the Save of the other tabs; a chat
 * keeps the level it was last sent with, and another model's is chosen in that model's menu under a message box. */
export const ThinkingDefaultField = ({provider, model}: ThinkingDefaultFieldProps) => {
    const {levelFor, choose} = useThinkingDefaults()
    // null: not asked yet, or a model that doesn't think (no levels to choose from)
    const [levels, setLevels] = useState<string[] | null>(null)

    useEffect(() => {
        setLevels(null)
        if (!model) return
        let cancelled = false
        getThinkingCapability(undefined, {model, provider}).then(
            (capability) => {
                if (cancelled || capability.kind === 'unsupported') return
                setLevels(offeredLevels(capability.kind === 'graduated' ? capability.modes : null))
            },
            () => {},
        )
        return () => {
            cancelled = true
        }
    }, [provider, model])

    if (!model || !levels) return null
    const chosen = levelFor(provider, model)
    return (
        <SettingsRow label="Default thinking" help="The level a chat on the default model starts on, until you pick another in the chat. Each chat then keeps the one it was last sent with.">
            <Select values={[FIRST_LEVEL, ...levels]} selected={chosen != null && levels.includes(chosen) ? chosen : FIRST_LEVEL}
                    onChosen={(value) => void choose(provider, model, value === FIRST_LEVEL ? null : value)}/>
        </SettingsRow>
    )
};

export interface HfTokenFieldProps {
    hasToken: boolean
    /** Saves the token (empty clears it) */
    onSave: (token: string) => Promise<void>
}

/** The user's Hugging Face token, needed to download gated models; saved on its own, never shown again */
export const HfTokenField = ({hasToken, onSave}: HfTokenFieldProps) => {
    const [token, setToken] = useState('')

    return (
        <SettingsRow label="Hugging Face token"
                     help="Only needed to download gated models. Create one at huggingface.co/settings/tokens (read access).">
            <PasswordInput text={token} onChanged={setToken} placeholder={hasToken ? 'a token is set' : 'hf_…'}/>
            <Button variant="secondary" text={token.trim() ? 'Save' : 'Clear'} disabled={!token.trim() && !hasToken}
                    onClicked={() => onSave(token.trim()).then(() => setToken(''))}/>
        </SettingsRow>
    )
};

/** 24-hour or 12-hour times; applies at once and is kept in this browser, like the theme */
export const TimeFormatChoice = () => {
    const {timeFormat, setTimeFormat} = useTheme()
    const sample = new Date(2026, 0, 1, 14, 11, 12)
    return (
        <ChoiceGroup label="Time format" chosen={timeFormat} onChosen={(format) => setTimeFormat(format as TimeFormat)}
                     options={(['24h', '12h'] as const).map((format) => ({value: format, label: formatTime(sample, format)}))}/>
    )
};

export interface SwitchFieldProps {
    enabled: boolean
    onToggle: (enabled: boolean) => void
}

/** Notifications toggle */
export const NotificationsField = ({enabled, onToggle}: SwitchFieldProps) => (
    <SettingsRow label="Notifications" help="When a reply is ready and the page is not in view. Asks your browser for permission.">
        <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
    </SettingsRow>
);

/** Auto-confirm toggle — saved with the rest of the user's settings */
export const AutoConfirmField = ({enabled, onToggle}: SwitchFieldProps) => (
    <SettingsRow label="Auto-confirm tool permissions" accent={enabled}
                 help="Skips the confirmation prompt and automatically allows whatever the model asks to do — only turn this on if you trust it to run unsupervised.">
        <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
    </SettingsRow>
);

/** Trim-old-thinking toggle — saved with the rest of the user's settings */
export const TrimOldThinkingField = ({enabled, onToggle}: SwitchFieldProps) => (
    <SettingsRow label="Shorten old thinking in long chats" accent={enabled}
                 help="When a long chat nears the model's context limit, the model's earlier reasoning is cut to its last lines and the newest reasoning is kept longer. This frees room without summarizing, but the model may re-check things it had already worked out.">
        <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
    </SettingsRow>
);

/** Whether the model suggests, after a reply, what to write next */
export const SuggestNextField = ({enabled, onToggle}: SwitchFieldProps) => (
    <SettingsRow label="Suggest what to write next"
                 help="After a reply, the model writes one line of what you might send next. It shows faded in the empty message box; Tab (or a tap on it) puts it in the box. It costs about a second of the model after each reply, and it is skipped when the model would have to read the chat again.">
        <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
    </SettingsRow>
);

/** Whether new chats send the model its tools — each chat has its own switch afterwards */
export const UseToolsField = ({enabled, onToggle}: SwitchFieldProps) => (
    <SettingsRow label="Use tools in new chats" accent={!enabled}
                 help="Tools let the model read and write files, run commands and look things up. Their definitions take about 9,000 tokens of every request, so a small model or a small context window does better without them. Only new chats take this; a chat has its own switch in its menu.">
        <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
    </SettingsRow>
);

/** Whether the model learns things about the user from their chats: saved with the rest of the user's settings */
export const MemoryEnabledField = ({enabled, onToggle}: SwitchFieldProps) => (
    <SettingsRow label="Remember things about me"
                 help="When a chat is folded to save room, the model reads it once more and keeps what it learned about you, such as who you are and how you like things done, for new chats. It goes by your own words only, and everything it keeps is listed below for you to change or delete.">
        <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
    </SettingsRow>
);

export interface SliderFieldProps {
    label: string
    help: string
    value: number
    onChanged: (value: number) => void
    min: number
    max: number
    step: number
    /** What the value reads as beside the slider */
    shown: string
}

/** A number chosen on a slider, with its value beside it: saved with the rest of the user's settings */
export const SliderField = ({label, help, value, onChanged, min, max, step, shown}: SliderFieldProps) => (
    <SettingsRow label={label} help={help} stack>
        <Div className="settings-row__slider">
            <Slider value={value} onChanged={onChanged} min={min} max={max} step={step}/>
            <Label className="settings-row__slider-value" text={shown}/>
        </Div>
    </SettingsRow>
);

/** The debug switch of this browser: it applies at once and isn't part of the saved settings */
export const DebugField = ({enabled, onToggle}: SwitchFieldProps) => (
    <SettingsRow label="Debug output in this browser" accent={enabled}
                 help="Shows diagnostic details about what the app is doing: in the browser console, and on screen where a page offers them. For finding problems; it adds noise, so leave it off otherwise. It applies to this browser only, takes effect at once and is not part of the saved settings.">
        <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
    </SettingsRow>
);

export interface MaxTurnStepsFieldProps {
    text: string
    onChanged: (text: string) => void
}

/** How many model calls one turn may make before it is stopped and the model is asked to wrap up */
export const MaxTurnStepsField = ({text, onChanged}: MaxTurnStepsFieldProps) => {
    const parsed = parseMaxTurnSteps(text)
    return (
        <SettingsRow label="Step limit per turn" accent={!parsed.valid}
                     help={parsed.valid
                         ? 'A turn is a series of model calls and tool calls. At this many, the model is told to write down where it got to and the turn stops until you continue. Empty or 0 means no limit.'
                         : 'A whole number from 0 to 10,000.'}>
            <Input className="settings-row__number" text={text} onChanged={onChanged} placeholder="No limit"/>
        </SettingsRow>
    )
};
