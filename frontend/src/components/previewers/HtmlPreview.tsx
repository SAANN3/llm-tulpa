import { Div, Label } from '../primitives'
import type { PreviewerProps } from './types'
import { useTextContent } from './useTextContent'

/** Renders the HTML for real, but fully sandboxed — this is untrusted uploaded
 * content, so the iframe gets an empty `sandbox` attribute (blocks scripts, forms,
 * popups, same-origin access — everything) rather than trusting it the way the app
 * trusts its own markup. `srcDoc` needs no `blob:`/network URL at all: the content is
 * already fetched as plain text (same fetch every text-based previewer uses). */
export default function HtmlPreview({ file }: PreviewerProps) {
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
    <iframe
      srcDoc={content}
      sandbox=""
      title={file.file_name}
      style={{ width: '100%', height: '100%', border: 'none', display: 'block', background: 'white' }}
    />
  )
}
