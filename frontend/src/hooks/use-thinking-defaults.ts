import {useCallback, useEffect, useState} from 'react'
import {getThinkingDefaults, setThinkingDefault, type ThinkingDefault} from '../api/llm/thinking-defaults.ts'

/** The level each model starts on for this user, read once; `choose` saves one (null takes it away) and patches the list */
export const useThinkingDefaults = () => {
    const [defaults, setDefaults] = useState<ThinkingDefault[]>([])

    useEffect(() => {
        // Nothing to start from is the same as no choice made: the composer then falls back to the model's first level
        getThinkingDefaults().then(setDefaults, () => setDefaults([]))
    }, [])

    const levelFor = useCallback(
        (provider: string | undefined, model: string | undefined): string | null =>
            defaults.find((d) => d.provider === provider && d.model === model)?.level ?? null,
        [defaults],
    )

    const choose = async (provider: string, model: string, level: string | null) => {
        await setThinkingDefault(provider, model, level)
        setDefaults((now) => {
            const others = now.filter((d) => !(d.provider === provider && d.model === model))
            return level == null ? others : [...others, {provider, model, level}]
        })
    }

    return {levelFor, choose}
};
