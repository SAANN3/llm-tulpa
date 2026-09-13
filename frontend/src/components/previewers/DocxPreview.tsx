import axios from 'axios'
import { renderAsync } from 'docx-preview'
import { useEffect, useRef, useState } from 'react'

import { getFileDownloadUrl } from '../../api/files/download'
import { Div, Label } from '../primitives'
import type { PreviewerProps } from './types'

/** Word encodes most bullet-list markers as a character from a proprietary symbol
 * font (Wingdings/Wingdings2/Wingdings3/Webdings/Symbol — `w:sym` in the XML), not
 * real Unicode bullets. `docx-preview` renders that literally (the raw character, set
 * in that font-family) — correct on Windows, where those fonts exist, but on a system
 * without them (most Linux desktops, this server's own rendering environment) the
 * browser has no glyph for that codepoint and shows a fallback/tofu box instead
 * (reported as literally "F0B7 in a square" — that's the Wingdings bullet's Private
 * Use Area codepoint printed as a placeholder). Not something a real Wingdings font
 * would fix even if bundled (its glyphs aren't freely redistributable), so this
 * substitutes the handful of codepoints Word actually uses for common bullet/arrow/
 * check markers with a real Unicode equivalent instead, post-render — anything from
 * one of these fonts that isn't in the map still falls back to a plain bullet rather
 * than a broken glyph, since in practice that's almost always what it actually was. */
const SYMBOL_FONTS = ['wingdings', 'wingdings2', 'wingdings3', 'webdings', 'symbol']
const SYMBOL_GLYPH_MAP: Record<string, string> = {
  '': '•', // solid round bullet -> •
  '': '▪', // solid square bullet -> ▪
  '': '→', // arrow -> →
  '': '➤', // arrow -> ➤
  '': '✔', // check -> ✔
  '': '✓', // check -> ✓
  '': '◆', // diamond -> ◆
  '': '◆', // diamond -> ◆
}

function fixSymbolFontGlyphs(container: HTMLElement): void {
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
}

/** Renders a `.docx` for real via `docx-preview` (parses the actual document XML and
 * lays it out as real DOM, not an image/screenshot) into a plain container div this
 * owns. White background regardless of theme — a document rendered at all its own
 * styling (fonts, colors assuming a white page) reads oddly against a dark app theme
 * otherwise. */
export default function DocxPreview({ file }: PreviewerProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [loading, setLoading] = useState(true)
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setFailed(false)

    axios
      .get<Blob>(getFileDownloadUrl(file.id), { responseType: 'blob' })
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
      <Div style={{ padding: 20 }}>
        <Label text="Couldn't render this document." />
      </Div>
    )
  }

  return (
    <Div style={{ background: 'white', minHeight: loading ? 0 : undefined, padding: loading ? 20 : 0 }}>
      {loading ? <Label variant="secondary" text="Loading…" /> : null}
      <div ref={containerRef} />
    </Div>
  )
}
