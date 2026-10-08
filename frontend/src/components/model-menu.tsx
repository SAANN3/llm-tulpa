import {useCallback, useEffect, useRef, useState} from 'react'
import '../styles/model-menu.scss'
import {getRecentModels} from '../api/chats/recent-models'
import type {RecentModelOut} from '../api/chats/types'
import type {LaunchProfile} from '../api/profiles/types'
import {useProfileCatalog} from '../hooks/use-profile-catalog.ts'
import {useServerEvent} from '../hooks/use-server-events.ts'
import {sameChoice, type ModelChoice} from '../utils/model-choice.ts'
import {shortModelName} from '../utils/model-name.ts'
import {profileSummary} from '../utils/profile-summary.ts'
import {ChooseModelPopup} from './popups/choose-model-popup.tsx'
import {Popup} from './popups/base/popup.tsx'
import {Button, Div, Label} from './primitives'

/** The thinking levels a model offers and the one chosen; `off` is always among them */
export interface ThinkingLevels {
    levels: string[]
    selected: string
}

export interface ModelMenuProps {
    /** The model the next message goes to; null while it is still loading */
    current: ModelChoice | null
    onPick: (choice: ModelChoice) => void
    /** Given for a pick that differs from the user's default (a new chat's model): the button marks it and the
     * menu offers the way back */
    onUseDefault?: () => void
    /** The user's default model, on a page where picking another one matters (a new chat's): its row is marked */
    defaultChoice?: ModelChoice | null
    /** Null for a model that doesn't think */
    thinking: ThinkingLevels | null
    onThinking: (level: string) => void
    disabled?: boolean
}

const RECENT_SHOWN = 3

const capitalize = (text: string) => text.charAt(0).toUpperCase() + text.slice(1)

/** What the button says about thinking: the level, or whether it thinks at all for an on/off model */
const thinkingText = (thinking: ThinkingLevels): string => {
    if (thinking.selected === 'off') return 'No think'
    return thinking.selected === 'on' ? 'Think' : capitalize(thinking.selected)
}

/** The composer's model button: the model's short name and its thinking level. Its menu lists the thinking levels
 * and the recently used models; every other model is in the Choose model popup behind "All models…". */
export const ModelMenu = ({current, onPick, onUseDefault, defaultChoice, thinking, onThinking, disabled}: ModelMenuProps) => {
    const {models, profiles} = useProfileCatalog()
    const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null)
    const [recent, setRecent] = useState<RecentModelOut[]>([])
    const [choosing, setChoosing] = useState(false)
    const anchorRef = useRef<HTMLDivElement>(null)

    const nameOf = (choice: ModelChoice): string => {
        const profile = profiles.find((p) => p.id === choice.profileId)
        const model = profile ? models.find((m) => m.id === profile.model_id) : null
        return model?.display_name ?? shortModelName(choice.model)
    }

    // A model with several profiles names the profile too, so its entries can be told apart
    const entryName = (choice: ModelChoice): string => {
        const profile = profiles.find((p) => p.id === choice.profileId)
        const model = profile ? models.find((m) => m.id === profile.model_id) : null
        return profile && (model?.profile_ids.length ?? 1) > 1 ? `${nameOf(choice)} · ${profile.name}` : nameOf(choice)
    }

    const entryDetail = (choice: ModelChoice): string | null => {
        if (choice.provider === 'ollama') return 'Ollama'
        const profile = profiles.find((p) => p.id === choice.profileId)
        return profile ? profileSummary(profile) : null
    }

    // Read when the page opens and again when a message is sent (a run starts), not on every pick: a chat saves a
    // pick at once, and reading then would reorder the list under the user before the pick was ever used
    const loadRecent = useCallback(() => {
        getRecentModels(RECENT_SHOWN).then(
            (result) => setRecent(result.models),
            () => {
                // Without the list the menu still offers the thinking levels and All models…
            },
        )
    }, [])
    useEffect(loadRecent, [loadRecent])
    useServerEvent('run_started', loadRecent)

    const open = () => {
        const rect = anchorRef.current?.getBoundingClientRect()
        if (!rect) return
        setMenuAt({x: rect.right, y: rect.top - 8})
    }

    // Only what the chats record, in their order: a pick the new chat hasn't been started with yet shows on the
    // button, not here, so the list moves only when a chat does
    const entries: ModelChoice[] = recent.map((r) => ({provider: r.provider, model: r.model, profileId: r.launch_profile_id}))

    const pick = (choice: ModelChoice) => {
        setMenuAt(null)
        setChoosing(false)
        if (!current || !sameChoice(choice, current)) onPick(choice)
    }

    const title = current
        ? current.model + (current.profileId != null ? ` · profile ${profiles.find((p) => p.id === current.profileId)?.name ?? ''}` : '')
            + (onUseDefault ? '\n* Only for this new chat; your default model stays as it is' : '')
        : undefined

    return (
        // The button's own press mustn't count as a click outside the open menu, or it would close the menu
        // and open it again in one click
        <Div ref={anchorRef} className="model-menu" onMouseDown={(e) => e.stopPropagation()}>
            <Button variant="secondary" className="model-menu__button" disabled={disabled || current == null} title={title}
                    onClicked={() => (menuAt ? setMenuAt(null) : open())}>
                <span className="model-menu__name">
                    {current ? nameOf(current) : '…'}
                    {onUseDefault ? <span className="model-menu__mark">*</span> : null}
                </span>
                {thinking ? (
                    <span className={`model-menu__level${thinking.selected === 'off' ? ' model-menu__level--off' : ''}`}>
                        {thinkingText(thinking)}
                    </span>
                ) : null}
            </Button>
            <Popup open={menuAt != null} onClose={() => setMenuAt(null)} position={menuAt ?? {x: 0, y: 0}} corner="bottom-right">
                <Div className="model-menu__panel">
                    <Label variant="secondary" className="model-menu__section" text="Thinking"/>
                    {thinking ? thinking.levels.map((level) => (
                        <Div key={level}
                             className={`model-menu__item${level === thinking.selected ? ' model-menu__item--on' : ''}`}
                             onClick={() => {
                                 setMenuAt(null)
                                 onThinking(level)
                             }}>
                            <Label text={capitalize(level)}/>
                        </Div>
                    )) : (
                        <Label variant="secondary" className="model-menu__note" text="This model doesn't think"/>
                    )}
                    <Div className="model-menu__divider"/>
                    {entries.length > 0 ? <Label variant="secondary" className="model-menu__section" text="Recent models"/> : null}
                    {entries.map((choice) => {
                        const detail = entryDetail(choice)
                        return (
                            <Div key={`${choice.provider}/${choice.model}/${choice.profileId}`}
                                 className={`model-menu__item${current && sameChoice(choice, current) ? ' model-menu__item--on' : ''}`}
                                 onClick={() => pick(choice)}>
                                <span className="model-menu__entry-name">
                                    <Label text={entryName(choice)}/>
                                    {defaultChoice && sameChoice(choice, defaultChoice) ? <span className="model-menu__mark">*</span> : null}
                                </span>
                                {detail ? <Label className="model-menu__detail" text={detail}/> : null}
                            </Div>
                        )
                    })}
                    {onUseDefault ? (
                        <Div className="model-menu__item" onClick={() => {
                            setMenuAt(null)
                            onUseDefault()
                        }}>
                            <Label text="Use default"/>
                        </Div>
                    ) : null}
                    <Div className="model-menu__item" onClick={() => {
                        setMenuAt(null)
                        setChoosing(true)
                    }}>
                        <Label text="All models…"/>
                    </Div>
                </Div>
            </Popup>
            <ChooseModelPopup
                open={choosing}
                provider={current?.provider ?? 'llama-cpp'}
                selected={current?.provider === 'ollama' ? current.model : null}
                selectedProfileId={current?.profileId ?? null}
                onSelect={(name) => pick({provider: 'ollama', model: name, profileId: null})}
                onSelectProfile={(profile: LaunchProfile) => pick({provider: profile.provider, model: profile.model, profileId: profile.id})}
                onClose={() => setChoosing(false)}
            />
        </Div>
    )
};
