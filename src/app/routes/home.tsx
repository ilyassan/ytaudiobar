import { useState, useEffect } from 'react'
import { AppHeader } from '@/components/app-header'
import { MiniPlayer } from '@/features/player/mini-player'
import { ExpandedPlayer } from '@/features/player/expanded-player'
import { SearchTab } from '@/features/search/search-tab'
import { QueueTab } from '@/features/queue/queue-tab'
import { PlaylistsTab } from '@/features/playlists/playlists-tab'
import { SettingsTab } from '@/features/settings/settings-tab'
import { checkYtdlpInstalled, installYtdlp, listenToPlaybackState, searchYoutube, type AudioState, type YTVideoInfo } from '@/lib/tauri'

type TabName = 'search' | 'queue' | 'playlists' | 'downloads' | 'settings'

export function HomePage() {
    const [activeTab, setActiveTab] = useState<TabName>('search')
    const [isExpanded, setIsExpanded] = useState(false)
    const [currentTrack, setCurrentTrack] = useState<YTVideoInfo | null>(null)
    const [isPlaying, setIsPlaying] = useState(false)
    const [audioState, setAudioState] = useState<AudioState | null>(null)
    const [isInitializing, setIsInitializing] = useState(true)

    // Search state (lifted from SearchTab to be accessible from Header)
    const [searchQuery, setSearchQuery] = useState('')
    const [isMusicMode, setIsMusicMode] = useState(false)
    const [searchResults, setSearchResults] = useState<YTVideoInfo[]>([])
    const [isSearching, setIsSearching] = useState(false)
    const [searchTimeout, setSearchTimeout] = useState<NodeJS.Timeout | null>(null)

    // Initialize YTDLP
    useEffect(() => {
        const initYtdlp = async () => {
            try {
                const isInstalled = await checkYtdlpInstalled()
                if (!isInstalled) {
                    await installYtdlp()
                }
                setIsInitializing(false)
            } catch (error) {
                console.error('Failed to initialize yt-dlp:', error)
                setIsInitializing(false)
            }
        }
        initYtdlp()
    }, [])

    // Listen to playback state changes
    useEffect(() => {
        const unlisten = listenToPlaybackState((state) => {
            setAudioState(state)
            setIsPlaying(state.is_playing)
            if (state.current_track) {
                setCurrentTrack(state.current_track)
            }
        })

        return () => {
            unlisten.then((fn) => fn())
        }
    }, [])

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
            setSearchResults([])
        }
    }, [searchQuery, isMusicMode])

    const performSearch = async (query: string) => {
        if (!query.trim()) return

        setIsSearching(true)
        // Auto-switch to search tab when user starts searching
        setActiveTab('search')
        try {
            const results = await searchYoutube(query, isMusicMode)
            setSearchResults(results)
        } catch (error) {
            console.error('Search failed:', error)
            setSearchResults([])
        } finally {
            setIsSearching(false)
        }
    }

    if (isInitializing) {
        return (
            <div className="flex h-screen items-center justify-center bg-background">
                <div className="text-center">
                    <div className="text-2xl mb-2">⏳</div>
                    <div className="text-sm text-muted-foreground">Initializing...</div>
                </div>
            </div>
        )
    }

    return (
        <div className="flex flex-col h-screen bg-background select-none">
            {/* Header - App Title + Search Bar */}
            <AppHeader
                query={searchQuery}
                onQueryChange={setSearchQuery}
                isMusicMode={isMusicMode}
                onMusicModeToggle={() => setIsMusicMode(!isMusicMode)}
            />

            {/* Player - appears below header when track is loaded */}
            {currentTrack && (
                <>
                    {!isExpanded ? (
                        <MiniPlayer
                            track={currentTrack}
                            isPlaying={isPlaying}
                            onExpand={() => setIsExpanded(true)}
                        />
                    ) : audioState && (
                        <ExpandedPlayer
                            audioState={audioState}
                            onCollapse={() => setIsExpanded(false)}
                        />
                    )}
                </>
            )}

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
                {activeTab === 'search' && (
                    <SearchTab
                        query={searchQuery}
                        isMusicMode={isMusicMode}
                        results={searchResults}
                        isSearching={isSearching}
                    />
                )}
                {activeTab === 'queue' && <QueueTab />}
                {activeTab === 'playlists' && <PlaylistsTab />}
                {activeTab === 'downloads' && (
                    <div className="flex items-center justify-center h-full">
                        <div className="text-center text-muted-foreground">
                            <p className="text-[15px] font-semibold mb-2">Downloads</p>
                            <p className="text-[13px]">Coming soon...</p>
                        </div>
                    </div>
                )}
                {activeTab === 'settings' && <SettingsTab />}
            </div>
        </div>
    )
}

export const Component = HomePage
