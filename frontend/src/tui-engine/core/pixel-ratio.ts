/**
 * Calls `onChange` whenever the screen's pixel density changes: the window moved to a monitor with another scale, or
 * the system scale was switched. Neither changes the size of what a canvas sits in, so a size observer never hears
 * of it and the canvas would stay drawn for the old density, blurred or too sharp. Returns the function that stops it.
 */
export function watchPixelRatio(onChange: () => void): () => void {
    let query: MediaQueryList
    // A query for one exact density fires once, when the density stops matching it, so it is made again after each change
    const handle = () => {
        onChange()
        listen()
    }
    const listen = () => {
        query = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
        query.addEventListener('change', handle, {once: true})
    }
    listen()
    return () => query.removeEventListener('change', handle)
}
