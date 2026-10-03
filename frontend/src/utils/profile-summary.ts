import type {LaunchProfile} from '../api/profiles/types'

/** What a launch profile does, in a line: the same wherever a profile is listed */
export const profileSummary = (p: LaunchProfile): string =>
    [
        p.context_length ? `${p.context_length} ctx` : 'auto ctx',
        `KV ${p.cache_type_k}/${p.cache_type_v}`,
        p.mtp ? 'MTP' : null,
        p.mmproj_file ? (p.mmproj_gpu ? 'vision' : 'vision in RAM') : null,
        p.gpu_layers < 99 ? `${p.gpu_layers} GPU layers` : null,
    ].filter(Boolean).join(' · ')
