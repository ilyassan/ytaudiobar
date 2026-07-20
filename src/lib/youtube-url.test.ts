import { describe, it, expect } from 'vitest'
import { extractYouTubeId, extractPlaylistUrl } from './youtube-url'

describe('extractYouTubeId', () => {
    it('extracts the id from a standard watch URL', () => {
        expect(extractYouTubeId('https://www.youtube.com/watch?v=abc123')).toBe(
            'abc123'
        )
    })

    it('extracts the id from a youtu.be short link', () => {
        expect(extractYouTubeId('https://youtu.be/abc123')).toBe('abc123')
    })

    it('strips query params from a youtu.be link', () => {
        expect(extractYouTubeId('https://youtu.be/abc123?t=30')).toBe('abc123')
    })

    it('extracts the id from a /shorts/ link', () => {
        expect(extractYouTubeId('https://www.youtube.com/shorts/xyz789')).toBe(
            'xyz789'
        )
    })

    it('extracts the id from a /shorts/ link with a trailing query param', () => {
        expect(
            extractYouTubeId(
                'https://www.youtube.com/shorts/xyz789?feature=share'
            )
        ).toBe('xyz789')
    })

    it('handles a video URL that also carries an incidental playlist param', () => {
        expect(
            extractYouTubeId(
                'https://www.youtube.com/watch?v=abc123&list=PLxyz'
            )
        ).toBe('abc123')
    })

    it('returns null for plain search text (not a URL)', () => {
        expect(extractYouTubeId('never gonna give you up')).toBeNull()
    })

    it('returns null for a non-YouTube URL', () => {
        expect(
            extractYouTubeId('https://example.com/watch?v=abc123')
        ).toBeNull()
    })

    it('returns null for a YouTube URL with no video id', () => {
        expect(
            extractYouTubeId(
                'https://www.youtube.com/results?search_query=cats'
            )
        ).toBeNull()
    })

    it('trims surrounding whitespace before parsing', () => {
        expect(extractYouTubeId('  https://youtu.be/abc123  ')).toBe('abc123')
    })

    it('returns null for an empty string', () => {
        expect(extractYouTubeId('')).toBeNull()
    })
})

describe('extractPlaylistUrl', () => {
    it('matches a dedicated playlist link', () => {
        const url = 'https://www.youtube.com/playlist?list=PLxyz'
        expect(extractPlaylistUrl(url)).toBe(url)
    })

    it('does NOT match a /watch link that merely carries an incidental list param', () => {
        expect(
            extractPlaylistUrl(
                'https://www.youtube.com/watch?v=abc123&list=PLxyz'
            )
        ).toBeNull()
    })

    it('returns null when the playlist link has no list param', () => {
        expect(
            extractPlaylistUrl('https://www.youtube.com/playlist')
        ).toBeNull()
    })

    it('returns null for plain search text', () => {
        expect(extractPlaylistUrl('just some search text')).toBeNull()
    })

    it('returns null for a non-YouTube domain even with a matching path', () => {
        expect(
            extractPlaylistUrl('https://example.com/playlist?list=PLxyz')
        ).toBeNull()
    })

    it('trims surrounding whitespace before parsing', () => {
        const url = 'https://www.youtube.com/playlist?list=PLxyz'
        expect(extractPlaylistUrl(`  ${url}  `)).toBe(url)
    })
})
