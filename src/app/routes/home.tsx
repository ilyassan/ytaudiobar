import { useState, useEffect, useRef } from 'react'
import { Loader2, ArrowLeft, AlertCircle, X } from 'lucide-react'
import { AppHeader } from '@/components/app-header'
import { DependencyLoader } from '@/components/dependency-loader'
import { ToastContainer } from '@/components/toast-container'
import { MiniPlayer } from '@/features/player/mini-player'
import { ExpandedPlayer } from '@/features/player/expanded-player'
import { SearchTab } from '@/features/search/search-tab'
import { PlaylistPreview } from '@/features/search/playlist-preview'
import { QueueTab } from '@/features/queue/queue-tab'
import { PlaylistsTab } from '@/features/playlists/playlists-tab'
import { DownloadsTab } from '@/features/downloads/downloads-tab'
import { SettingsTab } from '@/features/settings/settings-tab'
import { usePlayerStore } from '@/stores/player-store'
import { useDownloadsStore } from '@/stores/downloads-store'
import { useFavoritesStore } from '@/stores/favorites-store'
import { useToastStore } from '@/stores/toast-store'
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts'
import { useMediaKeys } from '@/hooks/useMediaKeys'
import { extractYouTubeId, extractPlaylistUrl } from '@/lib/youtube-url'
import {
    checkYtdlpInstalled,
    installYtdlp,
    checkFfmpegAvailable,
    installFfmpeg,
    listenToDepProgress,
    listenToPlaybackState,
    listenToDownloadsUpdate,
    searchYoutube,
    searchPlaylists,
    getPlaylistPreview,
    cancelSearch,
    getVideoInfoFast,
    togglePlayPause,
    seekTo,
    updateMediaPlaybackState,
    clearMediaInfo,
    type AudioState,
    type YTVideoInfo,
    type YTPlaylistInfo,
    type YTPlaylistPreview
} from '@/lib/tauri'
import { invoke } from '@tauri-apps/api/core'

type TabName = 'search' | 'queue' | 'playlists' | 'downloads' | 'settings'

// How many playback-state updates an optimistic seek may stay unconverged
// before we stop overriding the reported position. State is emitted about
// twice a second, so this is a ~5s grace period -- long enough for a real
// seek (which has to respawn ffmpeg) to land, short enough that a dropped
// one doesn't leave the progress bar stuck.
const SEEK_LATCH_MAX_TICKS = 10

export function HomePage() {
    const [activeTab, setActiveTab] = useState<TabName>('search')
    const [isExpanded, setIsExpanded] = useState(false)
    const [currentTrack, setCurrentTrack] = useState<YTVideoInfo | null>(null)
    const [isPlaying, setIsPlaying] = useState(false)
    const [audioState, setAudioState] = useState<AudioState | null>(null)
    // Shared between the playback-state-changed listener below and
    // useKeyboardShortcuts, which merges backend state against an in-flight
    // optimistic seek position -- owned here since both places need it.
    const positionRef = useRef(0) // Local position for keyboard seeking (ref = no stale closures)
    const targetSeekRef = useRef<number | null>(null) // Target seek position (ref = always latest in listener)
    // Escape hatches for the optimistic-seek latch below: a seek can be
    // silently dropped by the backend (e.g. it arrives while the next track is
    // still resolving), and without these the latch would never clear and the
    // displayed position would stay frozen at the target forever.
    const seekLatchTrackIdRef = useRef<string | null>(null)
    const seekLatchTicksRef = useRef(0)
    const [playbackError, setPlaybackError] = useState<string | null>(null)
    const lastShownErrorRef = useRef<string | null>(null)
    const errorDismissTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
        null
    )
    const [isInitializing, setIsInitializing] = useState(true)
    const [loadingStatus, setLoadingStatus] = useState<
        'checking' | 'downloading-ytdlp' | 'downloading-ffmpeg' | 'complete'
    >('checking')
    const [loadingProgress, setLoadingProgress] = useState(0)
    const needsYtdlpRef = useRef(false)
    const needsFfmpegRef = useRef(false)

    // Get Zustand store actions via selectors instead of destructuring the whole
    // store, so this component doesn't also subscribe to state it doesn't read
    // (currentTrack/isPlaying are tracked locally here via audioState instead).
    const setStoreTrack = usePlayerStore((s) => s.setCurrentTrack)
    const setStorePlaying = usePlayerStore((s) => s.setIsPlaying)
    const setLoadingTrack = usePlayerStore((s) => s.setLoadingTrack)

    // If the track has ended, replay from beginning instead of resuming at the end
    const handleTogglePlayPause = async () => {
        try {
            if (
                audioState &&
                !audioState.is_playing &&
                audioState.duration > 0 &&
                audioState.current_position >= audioState.duration - 0.5
            ) {
                await seekTo(0) // backend seek auto-resumes playback
                return
            }
            await togglePlayPause()
        } catch (error) {
            console.error('Failed to toggle play/pause:', error)
            useToastStore.getState().show('Failed to play/pause')
        }
    }

    // Search state (lifted from SearchTab to be accessible from Header)
    const [searchQuery, setSearchQuery] = useState('')
    const [isPlaylistMode, setIsPlaylistMode] = useState(false)
    const [isShrinked, setIsShrinked] = useState(false)
    const [searchResults, setSearchResults] = useState<YTVideoInfo[]>([])
    const [playlistResults, setPlaylistResults] = useState<YTPlaylistInfo[]>([])
    const [playlistPreview, setPlaylistPreview] =
        useState<YTPlaylistPreview | null>(null)
    const [isSearching, setIsSearching] = useState(false)
    const [isLoadingPreview, setIsLoadingPreview] = useState(false)
    const [previewError, setPreviewError] = useState<string | null>(null)
    const [searchTimeout, setSearchTimeout] = useState<NodeJS.Timeout | null>(
        null
    )
    const searchRequestIdRef = useRef(0) // Track current search request to cancel stale requests

    // Initialize dependencies (yt-dlp + ffmpeg)
    useEffect(() => {
        const initDependencies = async () => {
            try {
                // Check what needs installing
                const ytdlpInstalled = await checkYtdlpInstalled()
                const ffmpegAvailable = await checkFfmpegAvailable()

                needsYtdlpRef.current = !ytdlpInstalled
                needsFfmpegRef.current = !ffmpegAvailable

                // If everything is already installed, skip immediately
                if (ytdlpInstalled && ffmpegAvailable) {
                    setIsInitializing(false)
                    return
                }

                // Listen for real download progress events
                const unlisten = await listenToDepProgress((progress) => {
                    if (progress.total === 0) return

                    const depPercent =
                        (progress.downloaded / progress.total) * 100
                    const bothNeeded =
                        needsYtdlpRef.current && needsFfmpegRef.current

                    if (progress.dependency === 'ytdlp') {
                        // yt-dlp: 0-50% if both needed, 0-100% if only ytdlp
                        const overall = bothNeeded
                            ? depPercent * 0.5
                            : depPercent
                        setLoadingProgress(overall)
                    } else if (progress.dependency === 'ffmpeg') {
                        // ffmpeg: 50-100% if both needed, 0-100% if only ffmpeg
                        const overall = bothNeeded
                            ? 50 + depPercent * 0.5
                            : depPercent
                        setLoadingProgress(overall)
                    }
                })

                // Install yt-dlp if needed
                if (!ytdlpInstalled) {
                    setLoadingStatus('downloading-ytdlp')
                    await installYtdlp()
                }

                // Install ffmpeg if needed
                if (!ffmpegAvailable) {
                    setLoadingStatus('downloading-ffmpeg')
                    await installFfmpeg()
                }

                unlisten()
                setIsInitializing(false)
            } catch (error) {
                console.error('Failed to initialize dependencies:', error)
                useToastStore
                    .getState()
                    .show(
                        'Failed to set up yt-dlp/ffmpeg — some features may not work'
                    )
                setIsInitializing(false)
            }
        }
        initDependencies()
    }, [])

    // Listen to playback state changes
    useEffect(() => {
        const unlisten = listenToPlaybackState((state) => {
            setIsPlaying(state.is_playing)
            setStorePlaying(state.is_playing)

            // A seek issued against a different track can never converge, so
            // don't let it hold the position display hostage.
            const trackId = state.current_track?.id ?? null
            if (
                targetSeekRef.current !== null &&
                seekLatchTrackIdRef.current !== null &&
                seekLatchTrackIdRef.current !== trackId
            ) {
                targetSeekRef.current = null
            }

            if (targetSeekRef.current !== null) {
                // We're waiting for backend to catch up to our target position
                if (
                    Math.abs(state.current_position - targetSeekRef.current) <
                    0.5
                ) {
                    // Backend caught up — accept real position
                    targetSeekRef.current = null
                    seekLatchTicksRef.current = 0
                    positionRef.current = state.current_position
                    setAudioState(state)
                } else if (seekLatchTicksRef.current >= SEEK_LATCH_MAX_TICKS) {
                    // The seek never landed -- the backend drops seeks that
                    // arrive while a track is still being resolved, and reports
                    // no error when it does. Give up and show the truth rather
                    // than freezing the progress bar at a position we never
                    // reached.
                    targetSeekRef.current = null
                    seekLatchTicksRef.current = 0
                    positionRef.current = state.current_position
                    setAudioState(state)
                } else {
                    // Backend is stale — merge state but keep our optimistic position
                    seekLatchTicksRef.current += 1
                    seekLatchTrackIdRef.current = trackId
                    positionRef.current = targetSeekRef.current
                    setAudioState({
                        ...state,
                        current_position: targetSeekRef.current
                    })
                }
            } else {
                seekLatchTicksRef.current = 0
                seekLatchTrackIdRef.current = trackId
                // No active seeking — accept backend state fully
                positionRef.current = state.current_position
                setAudioState(state)
            }

            if (state.current_track) {
                setCurrentTrack(state.current_track)
                setStoreTrack(state.current_track)
                // Update loading state based on backend
                if (state.is_loading) {
                    setLoadingTrack(state.current_track.id)
                } else {
                    setLoadingTrack(null)
                }
            }

            // Surface a playback failure once per occurrence, auto-dismissing after
            // a few seconds -- without the lastShownErrorRef guard this would re-fire
            // (and restart the dismiss timer) on every state tick while it's set.
            if (
                state.playback_error &&
                state.playback_error !== lastShownErrorRef.current
            ) {
                lastShownErrorRef.current = state.playback_error
                setPlaybackError(state.playback_error)
                if (errorDismissTimeoutRef.current) {
                    clearTimeout(errorDismissTimeoutRef.current)
                }
                errorDismissTimeoutRef.current = setTimeout(() => {
                    setPlaybackError(null)
                }, 6000)
            } else if (!state.playback_error) {
                lastShownErrorRef.current = null
            }
        })

        return () => {
            unlisten.then((fn) => fn())
        }
    }, [setStoreTrack, setStorePlaying, setLoadingTrack])

    // Load downloads state once, then keep it fresh from backend events instead of
    // polling — every TrackItem and the Downloads tab read from this shared store.
    useEffect(() => {
        void useDownloadsStore.getState().refresh()

        const unlisten = listenToDownloadsUpdate(() => {
            useDownloadsStore.getState().scheduleRefresh()
        })

        return () => {
            unlisten.then((fn) => fn())
        }
    }, [])

    // Same pattern as downloads: load favorites once, then refresh on the
    // "favorites-updated" event dispatched by track-item.tsx instead of every
    // consuming tab fetching and listening for it independently.
    useEffect(() => {
        void useFavoritesStore.getState().refresh()

        const handleFavoritesUpdate = () =>
            void useFavoritesStore.getState().refresh()
        window.addEventListener('favorites-updated', handleFavoritesUpdate)
        return () =>
            window.removeEventListener(
                'favorites-updated',
                handleFavoritesUpdate
            )
    }, [])

    // Update media info when track or playback state changes
    useEffect(() => {
        if (audioState && audioState.current_track) {
            // Note: updateMediaMetadata is now called from backend directly to avoid race conditions
            // Only update playback state from frontend
            updateMediaPlaybackState(
                audioState.is_playing,
                audioState.current_position,
                audioState.duration
            ).catch(console.error)
        } else {
            clearMediaInfo().catch(console.error)
        }
    }, [audioState])

    // Load mini mode from Rust DB on mount
    useEffect(() => {
        invoke<boolean>('get_mini_mode')
            .then((isMini) => {
                if (isMini) setIsShrinked(true)
            })
            .catch(console.error)
    }, [])

    // Collapse expanded player only when entering shrink mode
    useEffect(() => {
        if (isShrinked) setIsExpanded(false)
    }, [isShrinked])

    // Resize window when expanding/collapsing in shrink mode
    useEffect(() => {
        if (!isShrinked) return
        invoke('resize_window', { height: isExpanded ? 280.0 : 100.0 })
    }, [isExpanded])

    useMediaKeys(isPlaying, audioState)

    useKeyboardShortcuts({
        audioState,
        setAudioState,
        positionRef,
        targetSeekRef
    })

    // Handle search with debounce
    useEffect(() => {
        if (searchQuery.trim()) {
            // Debounce search
            if (searchTimeout) clearTimeout(searchTimeout)
            const timeout = setTimeout(() => {
                performSearch(searchQuery)
            }, 500)
            setSearchTimeout(timeout)

            return () => clearTimeout(timeout)
        } else {
            // Clear results and cancel any pending searches when query is empty
            cancelSearch().catch(console.error)
            searchRequestIdRef.current += 1
            setSearchResults([])
            setPlaylistResults([])
            setPlaylistPreview(null)
            setPreviewError(null)
            setIsSearching(false)
            setIsLoadingPreview(false)
        }
    }, [searchQuery, isPlaylistMode])

    const loadPlaylistPreview = async (playlistUrl: string) => {
        searchRequestIdRef.current += 1
        const currentRequestId = searchRequestIdRef.current

        setIsLoadingPreview(true)
        setActiveTab('search')
        setPlaylistPreview(null)
        setPreviewError(null)

        try {
            const preview = await getPlaylistPreview(playlistUrl)
            if (searchRequestIdRef.current === currentRequestId) {
                setPlaylistPreview(preview)
                setIsLoadingPreview(false)
            }
        } catch (error) {
            if (searchRequestIdRef.current === currentRequestId) {
                console.error('Failed to load playlist preview:', error)
                setPreviewError(
                    typeof error === 'string'
                        ? error
                        : "Couldn't load this playlist. It may be private, deleted, or unavailable."
                )
                setIsLoadingPreview(false)
            }
        }
    }

    const handleBackFromPreview = () => {
        // Invalidate any in-flight preview fetch so a slow response can't
        // clobber state after the user has already backed out
        searchRequestIdRef.current += 1
        setIsLoadingPreview(false)
        setPreviewError(null)
        setPlaylistPreview(null)
    }

    const performSearch = async (query: string) => {
        if (!query.trim()) return

        // Cancel any ongoing search on the backend
        await cancelSearch().catch(console.error)

        setPlaylistPreview(null)
        setPreviewError(null)

        // A pasted playlist link always previews the playlist, regardless of mode
        const playlistUrl = extractPlaylistUrl(query)
        if (playlistUrl) {
            await loadPlaylistPreview(playlistUrl)
            return
        }

        // Increment request ID to invalidate previous requests
        searchRequestIdRef.current += 1
        const currentRequestId = searchRequestIdRef.current

        setIsSearching(true)
        setActiveTab('search')

        // Detect single-video YouTube URL — fetch that specific track directly
        const videoId = extractYouTubeId(query)
        if (videoId) {
            try {
                const track = await getVideoInfoFast(videoId)
                if (searchRequestIdRef.current === currentRequestId) {
                    setSearchResults([track])
                    setIsSearching(false)
                }
            } catch (error) {
                if (searchRequestIdRef.current === currentRequestId) {
                    console.error('Failed to fetch video info:', error)
                    useToastStore.getState().show('Failed to load that video')
                    setSearchResults([])
                    setIsSearching(false)
                }
            }
            return
        }

        if (isPlaylistMode) {
            try {
                const playlists = await searchPlaylists(query)
                if (searchRequestIdRef.current === currentRequestId) {
                    setPlaylistResults(playlists)
                    setIsSearching(false)
                }
            } catch (error) {
                if (searchRequestIdRef.current === currentRequestId) {
                    console.error('Playlist search failed:', error)
                    useToastStore.getState().show('Playlist search failed')
                    setPlaylistResults([])
                    setIsSearching(false)
                }
            }
            return
        }

        try {
            const results = await searchYoutube(query)

            // Only use results if this is still the current request
            if (searchRequestIdRef.current === currentRequestId) {
                console.log(
                    `⚡ Fast search completed in request #${currentRequestId} with ${results.length} results (durations loading...)`
                )

                // Show results immediately (duration will be 0 initially)
                setSearchResults(results)
                setIsSearching(false)

                // Durations will be fetched on-demand when items become visible
            } else {
                console.log(
                    `🚫 Ignoring stale search request #${currentRequestId} (current: #${searchRequestIdRef.current})`
                )
            }
        } catch (error) {
            // Only handle error if this is still the current request
            if (searchRequestIdRef.current === currentRequestId) {
                console.error('Search failed:', error)
                useToastStore.getState().show('Search failed')
                setSearchResults([])
                setIsSearching(false)
            } else {
                console.log(
                    `🚫 Ignoring error from stale search request #${currentRequestId}`
                )
            }
        }
    }

    if (isInitializing) {
        return (
            <DependencyLoader
                status={loadingStatus}
                progress={loadingProgress}
            />
        )
    }

    return (
        <div
            className={`
            flex flex-col bg-background select-none rounded-[12px] overflow-hidden border border-white/10
            ${isShrinked ? '' : 'h-screen'}
        `}
        >
            <ToastContainer />

            {/* Header - App Title + Search Bar */}
            <AppHeader
                query={searchQuery}
                onQueryChange={setSearchQuery}
                isPlaylistMode={isPlaylistMode}
                isShrinked={isShrinked}
                onPlaylistModeToggle={() => setIsPlaylistMode(!isPlaylistMode)}
                onIsShrinkedToggle={() => {
                    const newShrinked = !isShrinked
                    setIsShrinked(newShrinked)
                    invoke('resize_window', {
                        height: newShrinked ? 100.0 : 500.0
                    })
                    invoke('set_mini_mode', { isMini: newShrinked })
                }}
                onResetWindow={() => {
                    const height = isShrinked
                        ? isExpanded
                            ? 280.0
                            : 100.0
                        : 500.0
                    invoke('reset_window', { height })
                }}
            />

            {/* Playback error banner - auto-dismisses after a few seconds */}
            {playbackError && (
                <div className="flex items-center gap-2 px-3 py-2 bg-[var(--macos-red)]/10 border-b border-macos-separator">
                    <AlertCircle className="w-4 h-4 text-macos-red flex-shrink-0" />
                    <p className="flex-1 text-[12px] text-foreground min-w-0 truncate">
                        {playbackError}
                    </p>
                    <button
                        onClick={() => setPlaybackError(null)}
                        className="w-5 h-5 flex items-center justify-center rounded-full hover-macos-button flex-shrink-0"
                        aria-label="Dismiss"
                        title="Dismiss"
                    >
                        <X className="w-3.5 h-3.5 text-muted-foreground" />
                    </button>
                </div>
            )}

            {/* Empty state in mini mode */}
            {isShrinked && !currentTrack && (
                <div className="flex items-center justify-center py-3">
                    <p className="text-[12px] text-muted-foreground">
                        Play something to see it here
                    </p>
                </div>
            )}

            {/* Player - appears below header when track is loaded */}
            {currentTrack && (
                <>
                    {!isExpanded ? (
                        <MiniPlayer
                            track={currentTrack}
                            isPlaying={isPlaying}
                            isLoading={audioState?.is_loading || false}
                            onExpand={() => setIsExpanded(true)}
                            onTogglePlayPause={handleTogglePlayPause}
                        />
                    ) : (
                        audioState && (
                            <div className={isShrinked ? 'pb-2' : ''}>
                                <ExpandedPlayer
                                    audioState={audioState}
                                    onCollapse={() => setIsExpanded(false)}
                                />
                            </div>
                        )
                    )}
                </>
            )}

            {!isShrinked && (
                <>
                    {/* Tab Navigation */}
                    <div className="flex border-b border-macos-separator bg-card flex-shrink-0">
                        <button
                            onClick={() => setActiveTab('search')}
                            className={`flex-1 py-2 text-[13px] font-medium transition-colors ${
                                activeTab === 'search'
                                    ? 'text-[var(--macos-blue)] border-b-2 border-[var(--macos-blue)]'
                                    : 'text-muted-foreground hover:text-foreground'
                            }`}
                        >
                            <span>Search</span>
                        </button>
                        <button
                            onClick={() => setActiveTab('queue')}
                            className={`flex-1 py-2 text-[13px] font-medium transition-colors ${
                                activeTab === 'queue'
                                    ? 'text-[var(--macos-blue)] border-b-2 border-[var(--macos-blue)]'
                                    : 'text-muted-foreground hover:text-foreground'
                            }`}
                        >
                            <span>Queue</span>
                        </button>
                        <button
                            onClick={() => setActiveTab('playlists')}
                            className={`flex-1 py-2 text-[13px] font-medium transition-colors ${
                                activeTab === 'playlists'
                                    ? 'text-[var(--macos-blue)] border-b-2 border-[var(--macos-blue)]'
                                    : 'text-muted-foreground hover:text-foreground'
                            }`}
                        >
                            <span>Playlists</span>
                        </button>
                        <button
                            onClick={() => setActiveTab('downloads')}
                            className={`flex-1 py-2 text-[13px] font-medium transition-colors ${
                                activeTab === 'downloads'
                                    ? 'text-[var(--macos-blue)] border-b-2 border-[var(--macos-blue)]'
                                    : 'text-muted-foreground hover:text-foreground'
                            }`}
                        >
                            <span>Downloads</span>
                        </button>
                        <button
                            onClick={() => setActiveTab('settings')}
                            className={`flex-1 py-2 text-[13px] font-medium transition-colors ${
                                activeTab === 'settings'
                                    ? 'text-[var(--macos-blue)] border-b-2 border-[var(--macos-blue)]'
                                    : 'text-muted-foreground hover:text-foreground'
                            }`}
                        >
                            <span>Settings</span>
                        </button>
                    </div>

                    {/* Tab Content */}
                    <div className="flex-1 overflow-hidden">
                        {activeTab === 'search' &&
                            (isLoadingPreview ? (
                                <div className="flex flex-col h-full bg-background">
                                    <div className="px-4 py-3 border-b border-macos-separator bg-card">
                                        <button
                                            onClick={handleBackFromPreview}
                                            className="w-8 h-8 flex items-center justify-center rounded-full hover-macos-button"
                                            aria-label="Back to search"
                                            title="Back to search"
                                        >
                                            <ArrowLeft className="w-5 h-5 text-foreground" />
                                        </button>
                                    </div>
                                    <div className="flex-1 flex flex-col items-center justify-center gap-3">
                                        <Loader2 className="w-6 h-6 text-muted-foreground animate-spin" />
                                        <div className="text-[13px] text-muted-foreground">
                                            Loading playlist...
                                        </div>
                                    </div>
                                </div>
                            ) : previewError ? (
                                <div className="flex flex-col h-full bg-background">
                                    <div className="px-4 py-3 border-b border-macos-separator bg-card">
                                        <button
                                            onClick={handleBackFromPreview}
                                            className="w-8 h-8 flex items-center justify-center rounded-full hover-macos-button"
                                            aria-label="Back to search"
                                            title="Back to search"
                                        >
                                            <ArrowLeft className="w-5 h-5 text-foreground" />
                                        </button>
                                    </div>
                                    <div className="flex-1 flex flex-col items-center justify-center text-center px-6 gap-3">
                                        <AlertCircle className="w-10 h-10 text-muted-foreground opacity-60" />
                                        <p className="text-[13px] text-muted-foreground max-w-[260px]">
                                            {previewError}
                                        </p>
                                    </div>
                                </div>
                            ) : playlistPreview ? (
                                <PlaylistPreview
                                    preview={playlistPreview}
                                    onBack={handleBackFromPreview}
                                    onPlayAll={() => setActiveTab('queue')}
                                />
                            ) : (
                                <SearchTab
                                    query={searchQuery}
                                    isPlaylistMode={isPlaylistMode}
                                    results={searchResults}
                                    playlistResults={playlistResults}
                                    isSearching={isSearching}
                                    onSelectPlaylist={(playlist) =>
                                        loadPlaylistPreview(playlist.url)
                                    }
                                />
                            ))}
                        {activeTab === 'queue' && <QueueTab />}
                        {activeTab === 'playlists' && (
                            <PlaylistsTab
                                onPlayAll={() => setActiveTab('queue')}
                            />
                        )}
                        {activeTab === 'downloads' && <DownloadsTab />}
                        {activeTab === 'settings' && <SettingsTab />}
                    </div>
                </>
            )}
        </div>
    )
}

export const Component = HomePage
