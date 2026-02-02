import { Search, Music } from 'lucide-react'
import { type YTVideoInfo } from '@/lib/tauri'
import { TrackItem } from '@/components/track-item'

interface SearchTabProps {
    query: string
    isMusicMode: boolean
    results: YTVideoInfo[]
    isSearching: boolean
}

export function SearchTab({ query, isMusicMode, results, isSearching }: SearchTabProps) {

    return (
        <div className="flex flex-col h-full overflow-y-auto bg-background">
            {!query ? (
                <div className="flex flex-col items-center justify-center h-full text-center px-6">
                    {isMusicMode ? (
                        <Music className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                    ) : (
                        <Search className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                    )}
                    <h3 className="text-[15px] font-semibold text-foreground mb-2">
                        {isMusicMode ? 'Search YouTube Music' : 'Search YouTube'}
                    </h3>
                    <p className="text-[13px] text-muted-foreground max-w-[250px]">
                        {isMusicMode
                            ? 'Find your favorite songs and music'
                            : 'Find your favorite songs and videos'}
                    </p>
                </div>
            ) : isSearching ? (
                <div className="flex items-center justify-center h-full">
                    <div className="text-[13px] text-muted-foreground">Searching for "{query}"...</div>
                </div>
            ) : results.length === 0 ? (
                <div className="flex items-center justify-center h-full">
                    <div className="text-[13px] text-muted-foreground">No results found</div>
                </div>
            ) : (
                <div className="py-2">
                    {results.map((track) => (
                        <TrackItem key={track.id} track={track} context="search" />
                    ))}
                </div>
            )}
        </div>
    )
}
