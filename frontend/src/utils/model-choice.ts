/** A model to run, with the launch profile it runs under (null for an Ollama model) */
export interface ModelChoice {
    provider: string
    model: string
    profileId: number | null
}

export const sameChoice = (a: ModelChoice, b: ModelChoice): boolean =>
    a.provider === b.provider && a.model === b.model && a.profileId === b.profileId
