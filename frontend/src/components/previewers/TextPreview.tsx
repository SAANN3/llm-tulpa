import { Div, Label } from '../primitives'
import type { PreviewerProps } from './types'
import { useTextContent } from './useTextContent'

/** Plain-text content, no highlighting — `.txt`/`.log`/anything else that's text but
 * not source code (see `CodePreview` for that). No explicit size of its own: the
 * popup shell (`WindowsPopup`) is the one resizable/scrollable box, so this just
 * renders at its natural content size within that. */
export default function TextPreview({ file }: PreviewerProps) {
  const { content, failed } = useTextContent(file)

  if (failed) {
    return (
      <Div style={{ padding: 20 }}>
        <Label text="Couldn't load this file's content." />
      </Div>
    )
  }

  if (content == null) {
    return (
      <Div style={{ padding: 20 }}>
        <Label variant="secondary" text="Loading…" />
      </Div>
    )
  }

  return (
    <Div style={{ padding: 12 }}>
      <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontFamily: 'monospace', fontSize: 13 }}>
        {content}
      </pre>
    </Div>
  )
}
