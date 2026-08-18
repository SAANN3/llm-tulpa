import { useEffect } from 'react'

/** Sets the browser tab's title to `title` for as long as the calling component is mounted. */
export function useDocumentTitle(title: string) {
  useEffect(() => {
    document.title = title
  }, [title])
}
