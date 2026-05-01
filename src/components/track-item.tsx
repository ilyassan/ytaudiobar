import { useState, useEffect, useRef } from 'react'
import {
    Play,
    Pause,
    Heart,
    Trash,
    Loader2,
    Music,
    Download,
    Link,
    ListPlus,
    ListEnd
} from 'lucide-react'
import {
    playTrack,
    togglePlayPause,
    addToQueue,
    addToQueueNext,
    downloadTrack,
    isTrackDownloaded,
    getActiveDownloads,
    type YTVideoInfo,
    type Track,
    formatDuration
} from '@/lib/tauri'
import {
    PlaylistSelectionModal,
    loadPlaylistsWithTrackData,
    type PlaylistWithData
} from '@/features/playlists/playlist-selection-modal'
import { Button } from '@/components/ui/button'
import { usePlayerStore } from '@/stores/player-store'

interface TrackItemProps {
    track: YTVideoInfo | Track
    context: 'search' | 'queue' | 'playlist'
    isCurrentTrack?: boolean
    isPlaying?: boolean
    isFavorite?: boolean
    onRemove?: () => void
    onToggleFavorite?: () => void
}

export function TrackItem({
    track,
    context,
    isCurrentTrack,
    isFavorite,
    onRemove,
    onToggleFavorite
}: TrackItemProps) {
    const [showPlaylistModal, setShowPlaylistModal] = useState(false)
    const [loadedPlaylists, setLoadedPlaylists] = useState<
        PlaylistWithData[] | null
    >(null)
    const [isDownloaded, setIsDownloaded] = useState(false)
    const [isDownloading, setIsDownloading] = useState(false)
    const [downloadProgress, setDownloadProgress] = useState<number>(0)
    const [isCheckingDownload, setIsCheckingDownload] = useState(true)
    const [contextMenu, setContextMenu] = useState<{
        x: number
        y: number
    } | null>(null)
    const contextMenuRef = useRef<HTMLDivElement>(null)

    const videoInfo: YTVideoInfo =
        'uploader' in track
            ? track
            : {
                  ...track,
                  uploader: track.author || 'Unknown',
                  audio_url: null,
                  description: null
              }

    const {
        loadingTrackId,
        currentTrack,
        isPlaying: globalIsPlaying,
        setLoadingTrack
    } = usePlayerStore()
    const isThisTrackLoading = loadingTrackId === videoInfo.id
    const isThisTrackPlaying =
        currentTrack?.id === videoInfo.id && globalIsPlaying

    useEffect(() => {
        const checkStatus = async () => {
            try {
                const downloaded = await isTrackDownloaded(videoInfo.id)
                setIsDownloaded(downloaded)

                if (!downloaded) {
                    const activeDownloads = await getActiveDownloads()
                    const thisDownload = activeDownloads.find(
                        (d) => d.video_id === videoInfo.id
                    )
                    if (thisDownload) {
                        setIsDownloading(true)
                        setDownloadProgress(thisDownload.progress)
                    }
                }
            } catch (error) {
                console.error('Failed to check download status:', error)
            } finally {
                setIsCheckingDownload(false)
            }
        }
        checkStatus()

        const interval = setInterval(async () => {
            try {
                const downloaded = await isTrackDownloaded(videoInfo.id)
                setIsDownloaded(downloaded)
                if (!downloaded) {
                    const activeDownloads = await getActiveDownloads()
                    const thisDownload = activeDownloads.find(
                        (d) => d.video_id === videoInfo.id
                    )
                    if (thisDownload) {
                        setIsDownloading(true)
                        setDownloadProgress(thisDownload.progress)
                    }
                }
            } catch (error) {
                console.error('Failed to check download status:', error)
            }
        }, 3000)
        return () => clearInterval(interval)
    }, [videoInfo.id])

    useEffect(() => {
        if (!isDownloading) return

        const checkProgress = async () => {
            try {
                const activeDownloads = await getActiveDownloads()
                const thisDownload = activeDownloads.find(
                    (d) => d.video_id === videoInfo.id
                )
                if (thisDownload) {
                    setDownloadProgress(thisDownload.progress)
                    if (thisDownload.is_completed) {
                        setIsDownloading(false)
                        setIsDownloaded(true)
                    }
                } else {
                    const downloaded = await isTrackDownloaded(videoInfo.id)
                    if (downloaded) {
                        setIsDownloading(false)
                        setIsDownloaded(true)
                    }
                }
            } catch (error) {
                console.error('Failed to check download progress:', error)
            }
        }

        checkProgress()
        const interval = setInterval(checkProgress, 500)
        return () => clearInterval(interval)
    }, [isDownloading, videoInfo.id])

    const handlePlay = async () => {
        if (isThisTrackLoading) return

        try {
            if (currentTrack?.id === videoInfo.id) {
                await togglePlayPause()
            } else {
                setLoadingTrack(videoInfo.id)
                await playTrack(videoInfo)
            }
        } catch (error) {
            console.error('Failed to play track:', error)
            setLoadingTrack(null)
        }
    }

    const handleAddToQueue = async (e?: React.MouseEvent) => {
        e?.stopPropagation()
        try {
            await addToQueue(videoInfo)
            setContextMenu(null)
        } catch (error) {
            console.error('Failed to add to queue:', error)
        }
    }

    const handlePlayNext = async () => {
        try {
            await addToQueueNext(videoInfo)
            setContextMenu(null)
        } catch (error) {
            console.error('Failed to add to queue next:', error)
        }
    }

    const handleRemoveFromQueueContext = () => {
        if (onRemove) {
            onRemove()
        }
        setContextMenu(null)
    }

    const handleToggleFavorite = async (e: React.MouseEvent) => {
        e.stopPropagation()
        if (onToggleFavorite) {
            onToggleFavorite()
        } else {
            try {
                const playlists = await loadPlaylistsWithTrackData(videoInfo.id)
                setLoadedPlaylists(playlists)
                setShowPlaylistModal(true)
            } catch (error) {
                console.error('Failed to load playlists:', error)
            }
        }
    }

    const handleDownload = async (e: React.MouseEvent) => {
        e.stopPropagation()
        if (isDownloaded || isDownloading) return

        setIsDownloading(true)
        try {
            await downloadTrack(videoInfo)
        } catch (error) {
            console.error('Failed to download track:', error)
            setIsDownloading(false)
        }
    }

    const handleContextMenu = (e: React.MouseEvent) => {
        e.preventDefault()
        setContextMenu({ x: e.clientX, y: e.clientY })
    }

    const handleCopyLink = async () => {
        const url = `https://www.youtube.com/watch?v=${videoInfo.id}`
        await navigator.clipboard.writeText(url)
        setContextMenu(null)
    }

    useEffect(() => {
        if (!contextMenu) return
        const handleClickOutside = (e: MouseEvent) => {
            if (
                contextMenuRef.current &&
                !contextMenuRef.current.contains(e.target as Node)
            ) {
                setContextMenu(null)
            }
        }
        document.addEventListener('mousedown', handleClickOutside)
        return () =>
            document.removeEventListener('mousedown', handleClickOutside)
    }, [contextMenu])

    return (
        <>
            <div
                className={`flex items-center gap-3 px-3 py-2 cursor-pointer transition-colors ${
                    context !== 'queue' ? 'hover-macos-button' : ''
                } ${isCurrentTrack ? 'bg-[var(--macos-blue)]/10' : ''}`}
                onClick={handlePlay}
                onContextMenu={handleContextMenu}
            >
                <div className="w-12 h-12 rounded flex-shrink-0 bg-secondary overflow-hidden">
                    {videoInfo.thumbnail_url ? (
                        <img
                            src={videoInfo.thumbnail_url}
                            alt={videoInfo.title}
                            className="w-full h-full object-cover"
                        />
                    ) : (
                        <div className="w-full h-full flex items-center justify-center">
                            <Music className="w-6 h-6 text-muted-foreground" />
                        </div>
                    )}
                </div>

                <div className="flex-1 min-w-0 overflow-hidden">
                    <div
                        className={`text-[15px] font-semibold truncate ${
                            isCurrentTrack
                                ? 'text-[var(--macos-blue)]'
                                : 'text-foreground'
                        }`}
                    >
                        {videoInfo.title}
                    </div>
                    <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
                        <span className="truncate">{videoInfo.uploader}</span>
                        {context !== 'search' && (
                            <>
                                <span>•</span>
                                <span className="flex-shrink-0 text-[11px]">
                                    {videoInfo.duration > 0
                                        ? formatDuration(videoInfo.duration)
                                        : '--:--'}
                                </span>
                            </>
                        )}
                    </div>
                </div>

                <div className="flex items-center gap-2 flex-shrink-0">
                    <button
                        onClick={(e) => {
                            e.stopPropagation()
                            handlePlay()
                        }}
                        disabled={isThisTrackLoading}
                        className="w-6 h-6 flex items-center justify-center hover-macos-button rounded disabled:cursor-not-allowed disabled:opacity-70"
                        title={
                            isThisTrackLoading
                                ? 'Loading...'
                                : isThisTrackPlaying
                                  ? 'Playing'
                                  : 'Play'
                        }
                    >
                        {isThisTrackLoading ? (
                            <Loader2 className="w-4 h-4 text-foreground animate-spin" />
                        ) : isThisTrackPlaying ? (
                            <Pause className="w-4 h-4 text-[var(--macos-blue)] fill-[var(--macos-blue)]" />
                        ) : (
                            <Play className="w-4 h-4 text-foreground fill-foreground" />
                        )}
                    </button>

                    {context !== 'queue' &&
                        !isDownloaded &&
                        !isCheckingDownload && (
                            <button
                                onClick={handleDownload}
                                className="w-6 h-6 flex items-center justify-center hover-macos-button rounded relative"
                                title={
                                    isDownloading
                                        ? `Downloading ${Math.round(downloadProgress * 100)}%`
                                        : 'Download'
                                }
                                disabled={isDownloading}
                            >
                                {isDownloading ? (
                                    <div className="relative w-6 h-6 flex items-center justify-center">
                                        <svg className="absolute w-6 h-6 -rotate-90">
                                            <circle
                                                cx="12"
                                                cy="12"
                                                r="10"
                                                stroke="currentColor"
                                                strokeWidth="2"
                                                fill="none"
                                                className="text-muted-foreground/30"
                                            />
                                            <circle
                                                cx="12"
                                                cy="12"
                                                r="10"
                                                stroke="currentColor"
                                                strokeWidth="2"
                                                fill="none"
                                                strokeDasharray={`${2 * Math.PI * 10}`}
                                                strokeDashoffset={`${2 * Math.PI * 10 * (1 - downloadProgress)}`}
                                                className="text-[var(--macos-blue)] transition-all duration-300"
                                                strokeLinecap="round"
                                            />
                                        </svg>
                                        <span className="text-[8px] font-bold text-[var(--macos-blue)]">
                                            {Math.round(downloadProgress * 100)}
                                        </span>
                                    </div>
                                ) : (
                                    <Download className="w-4 h-4 text-foreground" />
                                )}
                            </button>
                        )}

                    {context === 'playlist' && onRemove && (
                        <button
                            onClick={(e) => {
                                e.stopPropagation()
                                onRemove()
                            }}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded"
                            title="Remove"
                        >
                            <Trash className="w-4 h-4 text-macos-red" />
                        </button>
                    )}

                    {context !== 'playlist' && (
                        <button
                            onClick={handleToggleFavorite}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded group/heart"
                            title={
                                isFavorite
                                    ? 'Remove from Favorites'
                                    : 'Add to Favorites'
                            }
                        >
                            {isFavorite ? (
                                <Heart className="w-4 h-4 text-macos-red fill-[var(--macos-red)]" />
                            ) : (
                                <Heart className="w-4 h-4 text-foreground" />
                            )}
                        </button>
                    )}
                </div>
            </div>

            {contextMenu && (
                <div
                    ref={contextMenuRef}
                    className="fixed z-50 bg-card border border-white/10 rounded-lg shadow-xl py-1 min-w-[160px]"
                    style={{ top: contextMenu.y, left: contextMenu.x }}
                >
                    {context === 'queue' ? (
                        <Button
                            variant="ghost"
                            size="sm"
                            onClick={handleRemoveFromQueueContext}
                            className="w-full justify-start text-[13px] px-3"
                        >
                            <Trash className="mr-2 h-4 w-4 text-muted-foreground group-hover:text-red-500" />
                            Remove from Queue
                        </Button>
                    ) : (
                        <>
                            <Button
                                variant="ghost"
                                size="sm"
                                onClick={handlePlayNext}
                                className="w-full justify-start text-[13px] px-3"
                            >
                                <ListEnd className="mr-2 h-4 w-4 text-muted-foreground" />
                                Play Next
                            </Button>
                            <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => handleAddToQueue()}
                                className="w-full justify-start text-[13px] px-3"
                            >
                                <ListPlus className="mr-2 h-4 w-4 text-muted-foreground" />
                                Add to Queue
                            </Button>
                            {context === 'playlist' && onRemove && (
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    onClick={handleRemoveFromQueueContext}
                                    className="w-full justify-start text-[13px] px-3"
                                >
                                    <Trash className="mr-2 h-4 w-4 text-muted-foreground" />
                                    Remove from Playlist
                                </Button>
                            )}
                        </>
                    )}
                    <div className="h-[1px] bg-border my-1" />
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={handleCopyLink}
                        className="w-full justify-start text-[13px] px-3"
                    >
                        <Link className="mr-2 h-4 w-4 text-muted-foreground" />
                        Copy link
                    </Button>
                </div>
            )}

            {showPlaylistModal && loadedPlaylists && (
                <PlaylistSelectionModal
                    track={videoInfo}
                    initialPlaylists={loadedPlaylists}
                    onClose={() => {
                        setShowPlaylistModal(false)
                        setLoadedPlaylists(null)
                        if (onToggleFavorite) {
                            window.dispatchEvent(
                                new CustomEvent('favorites-updated')
                            )
                        }
                    }}
                />
            )}
        </>
    )
}
