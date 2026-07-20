// Pulled out of home.tsx's search handler so this URL-parsing logic can be
// unit-tested independently of the component that uses it.

export function extractYouTubeId(query: string): string | null {
    try {
        const url = new URL(query.trim())
        if (url.hostname === 'youtu.be')
            return url.pathname.slice(1).split('?')[0]
        if (url.hostname.endsWith('youtube.com')) {
            const v = url.searchParams.get('v')
            if (v) return v
            // Handle /shorts/VIDEO_ID
            const shortsMatch = url.pathname.match(/\/shorts\/([^/?]+)/)
            if (shortsMatch) return shortsMatch[1]
        }
    } catch {
        // Not a URL
    }
    return null
}

// Only matches a dedicated playlist link (youtube.com/playlist?list=...), not a
// /watch?v=X&list=Y video link that merely carries an incidental playlist param.
export function extractPlaylistUrl(query: string): string | null {
    try {
        const url = new URL(query.trim())
        if (
            url.hostname.endsWith('youtube.com') &&
            url.pathname === '/playlist' &&
            url.searchParams.get('list')
        ) {
            return url.toString()
        }
    } catch {
        // Not a URL
    }
    return null
}
