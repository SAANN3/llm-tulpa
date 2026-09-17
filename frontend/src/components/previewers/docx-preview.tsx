import axios from 'axios'
import {renderAsync} from 'docx-preview'
import {useEffect, useRef, useState} from 'react'
import {getFileDownloadUrl} from '../../api/files/download'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'

const SYMBOL_FONTS = ['wingdings', 'wingdings2', 'wingdings3', 'webdings', 'symbol']
const SYMBOL_GLYPH_MAP: Record<string, string> = {
    '': '•',
    '': '▪',
    '': '→',
    '': '➤',
    '': '✔',
    '': '✓',
    '': '◆',
    '': '◆',
}

/** Replaces Word's symbol-font bullet/arrow/check glyphs with real Unicode equivalents */
const fixSymbolFontGlyphs = (container: HTMLElement): void => {
    for (const span of container.querySelectorAll<HTMLElement>('span[style*="font-family" i]')) {
        const family = span.style.fontFamily.replace(/['"]/g, '').toLowerCase()
        if (!SYMBOL_FONTS.some((font) => family.includes(font))) continue

        const text = span.textContent ?? ''
        const fixed = [...text]
            .map((char) => SYMBOL_GLYPH_MAP[char] ?? (char.codePointAt(0)! >= 0xf000 ? '•' : char))
            .join('')

        if (fixed !== text) {
            span.textContent = fixed
            span.style.fontFamily = ''
        }
    }
};

/** Renders a docx for real via docx-preview, laid out as real DOM rather than an image */
const DocxPreview = ({file}: PreviewerProps) => {
    const containerRef = useRef<HTMLDivElement>(null)
    const [loading, setLoading] = useState(true)
    const [failed, setFailed] = useState(false)

    useEffect(() => {
        let cancelled = false
        setLoading(true)
        setFailed(false)

        axios
            .get<Blob>(getFileDownloadUrl(file.id), {responseType: 'blob'})
            .then((res) => {
                if (cancelled || !containerRef.current) return
                containerRef.current.innerHTML = ''
                return renderAsync(res.data, containerRef.current).then(() => {
                    if (!cancelled && containerRef.current) fixSymbolFontGlyphs(containerRef.current)
                })
            })
            .catch(() => {
                if (!cancelled) setFailed(true)
            })
            .finally(() => {
                if (!cancelled) setLoading(false)
            })

        return () => {
            cancelled = true
        }
    }, [file.id])

    if (failed) {
        return (
            <Div style={{padding: 20}}>
                <Label text="Couldn't render this document."/>
            </Div>
        )
    }

    return (
        <Div style={{background: 'white', minHeight: loading ? 0 : undefined, padding: loading ? 20 : 0}}>
            {loading ? <Label variant="secondary" text="Loading…"/> : null}
            <div ref={containerRef}/>
        </Div>
    )
};

export default DocxPreview
