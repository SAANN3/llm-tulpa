/** A sampling preset; a null value is not sent to the model, so the server's own default applies */
export interface Preset {
    id: number
    /** The model it was made for, or null for any model */
    model_id: number | null
    name: string
    temperature: number | null
    top_p: number | null
    top_k: number | null
    min_p: number | null
    repeat_penalty: number | null
    presence_penalty: number | null
    seed: number | null
}

/** What a request sets on a preset: replaced as a whole */
export type PresetIn = Omit<Preset, 'id'>

export interface ExportedPreset extends Omit<PresetIn, 'model_id'> {
    model: { provider: string; name: string } | null
}

export interface PresetsFile {
    format: string
    version: number
    presets: ExportedPreset[]
}

export interface ImportResult {
    imported: number
    renamed: number
    any_model: number
}
