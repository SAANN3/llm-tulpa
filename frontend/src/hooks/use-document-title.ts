import {useEffect} from 'react'

/** Sets the browser tab's title for as long as the calling component is mounted */
export const useDocumentTitle = (title: string) => {
    useEffect(() => {
        document.title = title
    }, [title])
};
