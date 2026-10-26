/** The levels a composer offers for a model: its own and `off`, or just `on` and `off` for a model with no levels */
export const offeredLevels = (modes: string[] | null): string[] => (modes ? [...modes, 'off'] : ['on', 'off'])

/** The first of the levels, in the order given, that the model offers; null when it offers none of them */
export const firstOffered = (candidates: Array<string | null | undefined>, modes: string[] | null): string | null => {
    const offered = offeredLevels(modes)
    return candidates.find((level): level is string => level != null && offered.includes(level)) ?? null
};
