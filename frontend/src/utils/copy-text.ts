/** Copies `text` to the clipboard; false when the browser refused. `navigator.clipboard` only
 * exists in a secure context (https or localhost), and this app is also opened over plain http
 * on a LAN address, so that case falls back to the legacy `execCommand('copy')`. */
export const copyText = async (text: string): Promise<boolean> => {
    if (navigator.clipboard) {
        try {
            await navigator.clipboard.writeText(text)
            return true
        } catch {
            // Permission denied or the document isn't focused: the legacy path may still work
        }
    }

    const previouslyFocused = document.activeElement as HTMLElement | null
    const area = document.createElement('textarea')
    area.value = text
    area.style.position = 'fixed'
    area.style.opacity = '0'
    document.body.appendChild(area)
    area.select()
    try {
        return document.execCommand('copy')
    } finally {
        area.remove()
        previouslyFocused?.focus()
    }
};
