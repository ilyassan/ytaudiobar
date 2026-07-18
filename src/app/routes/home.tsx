import { useState, useEffect, useRef } from 'react'
import { Loader2, ArrowLeft, AlertCircle } from 'lucide-react'
import { AppHeader } from '@/components/app-header'
import { DependencyLoader } from '@/components/dependency-loader'
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
    playNext as playNextTrack,
    playPrevious as playPreviousTrack,
    seekTo,
    updateMediaPlaybackState,
    clearMediaInfo,
    listenToMediaKeyToggle,
    listenToMediaKeyNext,
    listenToMediaKeyPrevious,
    listenToMediaKeyPlay,
    listenToMediaKeyPause,
    listenToMediaKeySeek,
    listenToMediaKeySeekTo,
    type AudioState,
    type YTVideoInfo,
    type YTPlaylistInfo,
    type YTPlaylistPreview
} from '@/lib/tauri'
import { invoke } from '@tauri-apps/api/core'

type TabName = 'search' | 'queue' | 'playlists' | 'downloads' | 'settings'

export function HomePage() {
    const [activeTab, setActiveTab] = useState<TabName>('search')
    const [isExpanded, setIsExpanded] = useState(false)
    const [currentTrack, setCurrentTrack] = useState<YTVideoInfo | null>(null)
    const [isPlaying, setIsPlaying] = useState(false)
    const [audioState, setAudioState] = useState<AudioState | null>(null)
    const positionRef = useRef(0) // Local position for keyboard seeking (ref = no stale closures)
    const targetSeekRef = useRef<number | null>(null) // Target seek position (ref = always latest in listener)
    const [isInitializing, setIsInitializing] = useState(true)
    const [loadingStatus, setLoadingStatus] = useState<
        'checking' | 'downloading-ytdlp' | 'downloading-ffmpeg' | 'complete'
    >('checking')
    const [loadingProgress, setLoadingProgress] = useState(0)
    const needsYtdlpRef = useRef(false)
    const needsFfmpegRef = useRef(false)

    // Get Zustand store actions
    const {
        setCurrentTrack: setStoreTrack,
        setIsPlaying: setStorePlaying,
        setLoadingTrack
    } = usePlayerStore()

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

            if (targetSeekRef.current !== null) {
                // We're waiting for backend to catch up to our target position
                if (
                    Math.abs(state.current_position - targetSeekRef.current) <
                    0.5
                ) {
                    // Backend caught up — accept real position
                    targetSeekRef.current = null
                    positionRef.current = state.current_position
                    setAudioState(state)
                } else {
                    // Backend is stale — merge state but keep our optimistic position
                    positionRef.current = targetSeekRef.current
                    setAudioState({
                        ...state,
                        current_position: targetSeekRef.current
                    })
                }
            } else {
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
        invoke<boolean>('get_mini_mode').then((isMini) => {
            if (isMini) setIsShrinked(true)
        })
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

    // Listen to media key events
    useEffect(() => {
        const unlisteners: Promise<() => void>[] = []

        // Play/Pause/Toggle
        unlisteners.push(
            listenToMediaKeyToggle(() => {
                togglePlayPause().catch(console.error)
            })
        )

        unlisteners.push(
            listenToMediaKeyPlay(() => {
                if (!isPlaying) {
                    togglePlayPause().catch(console.error)
                }
            })
        )

        unlisteners.push(
            listenToMediaKeyPause(() => {
                if (isPlaying) {
                    togglePlayPause().catch(console.error)
                }
            })
        )

        // Next/Previous
        unlisteners.push(
            listenToMediaKeyNext(() => {
                playNextTrack().catch(console.error)
            })
        )

        unlisteners.push(
            listenToMediaKeyPrevious(() => {
                playPreviousTrack().catch(console.error)
            })
        )

        // Seeking
        unlisteners.push(
            listenToMediaKeySeek((offset) => {
                if (audioState) {
                    const newPosition = Math.max(
                        0,
                        Math.min(
                            audioState.current_position + offset,
                            audioState.duration
                        )
                    )
                    seekTo(newPosition).catch(console.error)
                }
            })
        )

        unlisteners.push(
            listenToMediaKeySeekTo((position) => {
                seekTo(position).catch(console.error)
            })
        )

        return () => {
            Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()))
        }
    }, [isPlaying, audioState])

    // Keyboard shortcuts - uses refs for position tracking to avoid stale closures
    useEffect(() => {
        const handleKeyDown = (e: KeyboardEvent) => {
            // Don't trigger if user is typing in an input/textarea
            const target = e.target as HTMLElement
            if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
                return
            }

            switch (e.key) {
                case ' ': // Space bar - toggle play/pause
                    e.preventDefault()
                    togglePlayPause().catch(console.error)
                    break
                case 'ArrowLeft': // Left arrow - seek backward 5s
                    e.preventDefault()
                    if (audioState) {
                        const newPosition = Math.max(0, positionRef.current - 5)
                        positionRef.current = newPosition
                        targetSeekRef.current = newPosition
                        setAudioState({
                            ...audioState,
                            current_position: newPosition
                        })
                        seekTo(newPosition).catch(console.error)
                    }
                    break
                case 'ArrowRight': // Right arrow - seek forward 5s
                    e.preventDefault()
                    if (audioState) {
                        const newPosition = Math.min(
                            audioState.duration,
                            positionRef.current + 5
                        )
                        positionRef.current = newPosition
                        targetSeekRef.current = newPosition
                        setAudioState({
                            ...audioState,
                            current_position: newPosition
                        })
                        seekTo(newPosition).catch(console.error)
                    }
                    break
            }
        }

        window.addEventListener('keydown', handleKeyDown)
        return () => window.removeEventListener('keydown', handleKeyDown)
    }, [audioState])

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

    const extractYouTubeId = (query: string): string | null => {
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
    const extractPlaylistUrl = (query: string): string | null => {
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
            flex flex-col bg-background select-none rounded-[12px] overflow-hidden border border-white/10 + ' ' +
            ${isShrinked ? '' : 'h-screen'}
        `}
        >
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
