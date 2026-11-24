import {memo} from 'react'

// Parse into an inert template. Strip markup that could
// navigate or embed another document before mounting any preview.
const previewDocument = (html: string): string => {
    const document = window.document.implementation.createHTMLDocument()
    const template = document.createElement('template')
    template.innerHTML = html
    template.content.querySelectorAll('base, meta, iframe, frame, object, embed, form, script').forEach(element => {
        element.remove()
    })
    // SVG links can navigate from inside the preview document as well.
    template.content.querySelectorAll('a, area').forEach(element => {
        element.removeAttribute('href')
        element.removeAttribute('xlink:href')
    })
    return `<!doctype html><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'">
<style>:root { color-scheme: light dark; } body { color: CanvasText; background: Canvas; }</style>
${template.innerHTML}`
}

export const HtmlPreview = memo(({html}: {html: string}) => (
    <iframe title="HTML output" sandbox="" referrerPolicy="no-referrer" tabIndex={-1} srcDoc={previewDocument(html)}/>
))
