import { useState, useEffect, useRef } from 'react'
import { ArrowLeft, Play, Download, ListPlus, Music } from 'lucide-react'
import {
    type YTPlaylistPreview as YTPlaylistPreviewData,
    playTrackList,
    importPlaylist,
    downloadTrack
} from '@/lib/tauri'
import { TrackItem } from '@/components/track-item'
import { useFavoritesStore } from '@/stores/favorites-store'
import { useToastStore } from '@/stores/toast-store'

interface PlaylistPreviewProps {
    preview: YTPlaylistPreviewData
    onBack: () => void
    onPlayAll: () => void
}

export function PlaylistPreview({
    preview,
    onBack,
    onPlayAll
}: PlaylistPreviewProps) {
    const [isSaving, setIsSaving] = useState(false)
    const [isDownloading, setIsDownloading] = useState(false)
    const [statusMessage, setStatusMessage] = useState('')
    const statusTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
    const favoriteTrackIds = useFavoritesStore((s) => s.favoriteTrackIds)

    useEffect(() => {
        return () => {
            if (statusTimeoutRef.current) {
                clearTimeout(statusTimeoutRef.current)
            }
        }
    }, [])

    // Replace any dismissal still pending from a previous action so it can't
    // wipe the message that was just set.
    const scheduleStatusDismiss = (delayMs: number) => {
        if (statusTimeoutRef.current) {
            clearTimeout(statusTimeoutRef.current)
        }
        statusTimeoutRef.current = setTimeout(
            () => setStatusMessage(''),
            delayMs
        )
    }

    const coverThumbnail =
        preview.tracks.find((t) => t.thumbnail_url)?.thumbnail_url ?? null
    const isTruncated = preview.track_count > preview.tracks.length

    const handlePlayAll = async () => {
        try {
            await playTrackList(preview.tracks)
            onPlayAll()
        } catch (error) {
            console.error('Failed to play playlist:', error)
            useToastStore.getState().show('Failed to play playlist')
        }
    }

    const handleSaveAsPlaylist = async () => {
        setIsSaving(true)
        setStatusMessage('')
        try {
            await importPlaylist(preview.title, preview.tracks)
            setStatusMessage('Saved to My Playlists')
        } catch (error) {
            console.error('Failed to save playlist:', error)
            setStatusMessage('Failed to save playlist')
        } finally {
            setIsSaving(false)
            scheduleStatusDismiss(3000)
        }
    }

    const handleDownloadAll = async () => {
        setIsDownloading(true)
        setStatusMessage('')
        try {
            const results = await Promise.allSettled(
                preview.tracks.map((track) => downloadTrack(track))
            )
            const failed = results.filter((r) => r.status === 'rejected').length
            setStatusMessage(
                failed > 0
                    ? `Downloads started (${failed} failed to start)`
                    : 'Downloads started'
            )
        } finally {
            setIsDownloading(false)
            scheduleStatusDismiss(4000)
        }
    }

    return (
        <div className="flex flex-col h-full bg-background">
            <div className="px-4 py-3 border-b border-macos-separator bg-card">
                <div className="flex items-center gap-3 mb-3">
                    <button
                        onClick={onBack}
                        className="w-8 h-8 flex items-center justify-center rounded-full hover-macos-button flex-shrink-0"
                        aria-label="Back to search"
                        title="Back to search"
                    >
                        <ArrowLeft className="w-5 h-5 text-foreground" />
                    </button>
                    <div className="w-12 h-12 rounded flex-shrink-0 bg-secondary overflow-hidden">
                        {coverThumbnail ? (
                            <img
                                src={coverThumbnail}
                                alt={preview.title}
                                className="w-full h-full object-cover"
                            />
                        ) : (
                            <div className="w-full h-full flex items-center justify-center">
                                <Music className="w-6 h-6 text-muted-foreground" />
                            </div>
                        )}
                    </div>
                    <div className="flex-1 min-w-0">
                        <h2 className="text-[15px] font-semibold text-foreground truncate">
                            {preview.title}
                        </h2>
                        <div className="text-[12px] text-muted-foreground truncate">
                            {preview.uploader} &bull; {preview.track_count}{' '}
                            track{preview.track_count === 1 ? '' : 's'}
                            {isTruncated &&
                                ` (showing first ${preview.tracks.length})`}
                        </div>
                    </div>
                </div>

                <div className="flex flex-col gap-2">
                    <div className="flex items-center gap-2">
                        <button
                            onClick={handlePlayAll}
                            className="flex-1 flex items-center justify-center gap-2 px-3 py-1.5 rounded-full bg-[var(--macos-blue)] text-white hover:opacity-90 transition-opacity"
                        >
                            <Play className="w-3.5 h-3.5 fill-white" />
                            <span className="text-[12px] font-medium">
                                Play All
                            </span>
                        </button>
                        <button
                            onClick={handleSaveAsPlaylist}
                            disabled={isSaving}
                            className="flex-1 flex items-center justify-center gap-2 px-3 py-1.5 rounded-full bg-secondary hover-macos-button transition-colors disabled:opacity-50"
                        >
                            <ListPlus className="w-3.5 h-3.5 text-foreground" />
                            <span className="text-[12px] font-medium text-foreground">
                                {isSaving ? 'Saving...' : 'Save as Playlist'}
                            </span>
                        </button>
                    </div>
                    <button
                        onClick={handleDownloadAll}
                        disabled={isDownloading}
                        className="w-full flex items-center justify-center gap-2 px-3 py-1.5 rounded-full bg-secondary hover-macos-button transition-colors disabled:opacity-50"
                    >
                        <Download className="w-3.5 h-3.5 text-foreground" />
                        <span className="text-[12px] font-medium text-foreground">
                            {isDownloading ? 'Starting...' : 'Download All'}
                        </span>
                    </button>
                </div>

                {statusMessage && (
                    <p className="text-[11px] text-muted-foreground mt-2">
                        {statusMessage}
                    </p>
                )}
            </div>

            <div className="flex-1 overflow-y-auto py-2">
                {preview.tracks.map((track) => (
                    <TrackItem
                        key={track.id}
                        track={track}
                        context="search"
                        isFavorite={favoriteTrackIds.has(track.id)}
                    />
                ))}
            </div>
        </div>
    )
}
