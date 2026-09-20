/** A byte count as a short human string: `2.3 GB`, `640 MB` */
export const formatBytes = (bytes: number): string => {
    if (bytes < 1e6) return `${Math.round(bytes / 1e3)} kB`
    if (bytes < 1e9) return `${Math.round(bytes / 1e6)} MB`
    return `${(bytes / 1e9).toFixed(1)} GB`
};
