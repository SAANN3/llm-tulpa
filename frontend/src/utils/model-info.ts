import type {LocalModel} from '../api/llm/models'

const BYTES_PER_PARAM: Record<string, number> = {
    q2: 0.4, q3: 0.45, q4: 0.55, q5: 0.65, q6: 0.75, q8: 1.06, f16: 2, f32: 4,
}

const parseParams = (size?: string | null): number | null => {
    if (!size) return null
    const match = /([\d.]+)\s*([bm])/i.exec(size)
    if (!match) return null
    const value = parseFloat(match[1])
    if (!Number.isFinite(value)) return null
    return match[2].toLowerCase() === 'm' ? value / 1000 : value
};

const bytesPerParam = (quant?: string | null): number => {
    if (!quant) return 0.6
    const key = quant.toLowerCase()
    for (const prefix of Object.keys(BYTES_PER_PARAM)) {
        if (key.startsWith(prefix)) return BYTES_PER_PARAM[prefix]
    }
    return 0.6
};

export const estimateMemoryGb = (model: LocalModel): number | null => {
    if (model.size) return Math.round((model.size / 1e9) * 1.2 * 10) / 10
    const params = parseParams(model.details?.parameter_size)
    if (params == null) return null
    const gb = params * bytesPerParam(model.details?.quantization_level) * 1.2
    return Math.round(gb * 10) / 10
};

export const modelRequirements = (model: LocalModel): string => {
    const parts: string[] = []
    if (model.details?.parameter_size) parts.push(model.details.parameter_size)
    if (model.details?.quantization_level) parts.push(model.details.quantization_level)
    const mem = estimateMemoryGb(model)
    if (mem != null) parts.push(`~${mem} GB`)
    return parts.join(' · ')
};
