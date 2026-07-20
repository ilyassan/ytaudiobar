import { useState, useEffect, useRef } from 'react'
import { Shuffle, Repeat, Repeat1, ListMusic, GripVertical } from 'lucide-react'
import {
    getQueue,
    toggleShuffle,
    cycleRepeatMode,
    reorderQueue,
    removeFromQueue,
    getShuffleMode,
    getRepeatMode,
    listenToQueueUpdate,
    type YTVideoInfo,
    type RepeatMode
} from '@/lib/tauri'
import { TrackItem } from '@/components/track-item'
import { TabHeader } from '@/components/tab-header'
import { useToastStore } from '@/stores/toast-store'
import {
    DndContext,
    closestCenter,
    PointerSensor,
    useSensor,
    useSensors,
    DragOverlay,
    type DragStartEvent,
    type DragEndEvent
} from '@dnd-kit/core'
import {
    SortableContext,
    verticalListSortingStrategy,
    useSortable,
    arrayMove
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'

function SortableTrackRow({
    track,
    index,
    onRemove
}: {
    track: YTVideoInfo
    index: number
    onRemove: () => void
}) {
    const {
        attributes,
        listeners,
        setNodeRef,
        transform,
        transition,
        isDragging
    } = useSortable({ id: track.id })

    const style = {
        transform: CSS.Transform.toString(transform),
        transition,
        opacity: isDragging ? 0.4 : 1,
        zIndex: isDragging ? 0 : ('auto' as const)
    }

    return (
        <div
            ref={setNodeRef}
            style={style}
            className="group flex items-center hover-macos-button rounded transition-colors touch-none"
            {...attributes}
            {...listeners}
        >
            <div className="w-5 flex-shrink-0 flex items-center justify-center self-stretch">
                <span className="text-[11px] text-muted-foreground">
                    {index + 1}
                </span>
            </div>
            <div className="flex-shrink-0 flex items-center justify-center self-stretch px-0.5 cursor-grab active:cursor-grabbing">
                <GripVertical className="w-3.5 h-3.5 text-muted-foreground" />
            </div>

            <div className="flex-1 min-w-0 overflow-hidden">
                <TrackItem track={track} context="queue" onRemove={onRemove} />
            </div>
        </div>
    )
}

function DragOverlayRow({
    track,
    index
}: {
    track: YTVideoInfo
    index: number
}) {
    return (
        <div className="flex items-center bg-card/95 rounded-lg shadow-lg border border-white/10 backdrop-blur-sm">
            <div className="w-5 flex-shrink-0 text-center">
                <span className="text-[11px] text-muted-foreground">
                    {index + 1}
                </span>
            </div>
            <div className="flex-shrink-0 px-0.5 cursor-grabbing">
                <GripVertical className="w-3.5 h-3.5 text-muted-foreground" />
            </div>
            <div className="flex-1 min-w-0 overflow-hidden">
                <TrackItem track={track} context="queue" />
            </div>
        </div>
    )
}

export function QueueTab() {
    const [queue, setQueue] = useState<YTVideoInfo[]>([])
    const [shuffleMode, setShuffleMode] = useState(false)
    const [repeatMode, setRepeatMode] = useState<RepeatMode>('Off')
    const [activeId, setActiveId] = useState<string | null>(null)
    const [isLoading, setIsLoading] = useState(true)
    const isMountedRef = useRef(true)
    const latestLoadRequestRef = useRef(0)
    const isDraggingRef = useRef(false)

    const sensors = useSensors(
        useSensor(PointerSensor, {
            activationConstraint: { distance: 5 }
        })
    )

    const loadQueue = async () => {
        if (isDraggingRef.current) return

        const requestId = ++latestLoadRequestRef.current

        try {
            const [queueData, shuffle, repeat] = await Promise.all([
                getQueue(),
                getShuffleMode(),
                getRepeatMode()
            ])

            if (
                !isMountedRef.current ||
                requestId !== latestLoadRequestRef.current ||
                isDraggingRef.current
            ) {
                return
            }

            setQueue(queueData)
            setShuffleMode(shuffle)
            setRepeatMode(repeat)
        } catch (error) {
            console.error('Failed to load queue:', error)
            useToastStore.getState().show('Failed to load queue')
        } finally {
            if (
                isMountedRef.current &&
                requestId === latestLoadRequestRef.current
            ) {
                setIsLoading(false)
            }
        }
    }

    useEffect(() => {
        isMountedRef.current = true
        void loadQueue()

        const unlisten = listenToQueueUpdate(() => {
            void loadQueue()
        })

        return () => {
            isMountedRef.current = false
            unlisten.then((fn) => fn())
        }
    }, [])

    const handleToggleShuffle = async () => {
        try {
            const enabled = await toggleShuffle()
            setShuffleMode(enabled)
        } catch (error) {
            console.error('Failed to toggle shuffle:', error)
            useToastStore.getState().show('Failed to toggle shuffle')
        }
    }

    const handleCycleRepeat = async () => {
        try {
            const mode = await cycleRepeatMode()
            setRepeatMode(mode)
        } catch (error) {
            console.error('Failed to cycle repeat mode:', error)
            useToastStore.getState().show('Failed to change repeat mode')
        }
    }

    const handleRemoveFromQueue = async (index: number) => {
        try {
            await removeFromQueue(index)
        } catch (error) {
            console.error('Failed to remove from queue:', error)
            useToastStore.getState().show('Failed to remove from queue')
        }
    }

    const handleDragStart = (event: DragStartEvent) => {
        setActiveId(event.active.id as string)
        isDraggingRef.current = true
    }

    const handleDragEnd = (event: DragEndEvent) => {
        const { active, over } = event
        setActiveId(null)
        isDraggingRef.current = false

        if (!over || active.id === over.id) return

        const oldIndex = queue.findIndex((t) => t.id === active.id)
        const newIndex = queue.findIndex((t) => t.id === over.id)
        if (oldIndex === -1 || newIndex === -1) return

        const newQueue = arrayMove(queue, oldIndex, newIndex)
        setQueue(newQueue)

        reorderQueue(newQueue).catch((error) => {
            console.error('Failed to reorder queue:', error)
            useToastStore.getState().show('Failed to reorder queue')
            void loadQueue()
        })
    }

    const handleDragCancel = () => {
        setActiveId(null)
        isDraggingRef.current = false
    }

    const activeTrack = activeId ? queue.find((t) => t.id === activeId) : null
    const activeIndex = activeId
        ? queue.findIndex((t) => t.id === activeId)
        : -1

    return (
        <div className="flex flex-col h-full bg-background">
            <TabHeader
                title="Queue"
                actions={
                    <>
                        <button
                            onClick={handleToggleShuffle}
                            className={`w-8 h-8 flex items-center justify-center rounded-full hover-macos-button transition-colors ${
                                shuffleMode
                                    ? 'text-[var(--macos-blue)]'
                                    : 'text-muted-foreground'
                            }`}
                            title={shuffleMode ? 'Shuffle On' : 'Shuffle Off'}
                        >
                            <Shuffle className="w-4 h-4" />
                        </button>
                        <button
                            onClick={handleCycleRepeat}
                            className={`w-8 h-8 flex items-center justify-center rounded-full hover-macos-button transition-colors ${
                                repeatMode !== 'Off'
                                    ? 'text-[var(--macos-blue)]'
                                    : 'text-muted-foreground'
                            }`}
                            title={`Repeat ${repeatMode}`}
                        >
                            {repeatMode === 'One' ? (
                                <Repeat1 className="w-4 h-4" />
                            ) : (
                                <Repeat className="w-4 h-4" />
                            )}
                        </button>
                    </>
                }
            />

            <div className="flex-1 overflow-y-auto">
                {isLoading ? null : queue.length === 0 ? (
                    <div className="flex flex-col items-center justify-center h-full text-center px-6">
                        <ListMusic className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                        <h3 className="text-[15px] font-semibold text-foreground mb-2">
                            Queue is Empty
                        </h3>
                        <p className="text-[13px] text-muted-foreground max-w-[250px]">
                            Use Play All on a playlist to add tracks to your
                            queue
                        </p>
                    </div>
                ) : (
                    <DndContext
                        sensors={sensors}
                        collisionDetection={closestCenter}
                        onDragStart={handleDragStart}
                        onDragEnd={handleDragEnd}
                        onDragCancel={handleDragCancel}
                    >
                        <SortableContext
                            items={queue.map((t) => t.id)}
                            strategy={verticalListSortingStrategy}
                        >
                            <div className="py-2">
                                {queue.map((track, index) => (
                                    <SortableTrackRow
                                        key={track.id}
                                        track={track}
                                        index={index}
                                        onRemove={() =>
                                            handleRemoveFromQueue(index)
                                        }
                                    />
                                ))}
                            </div>
                        </SortableContext>
                        <DragOverlay
                            dropAnimation={{
                                duration: 200,
                                easing: 'cubic-bezier(0.25, 1, 0.5, 1)'
                            }}
                        >
                            {activeTrack ? (
                                <DragOverlayRow
                                    track={activeTrack}
                                    index={activeIndex}
                                />
                            ) : null}
                        </DragOverlay>
                    </DndContext>
                )}
            </div>
        </div>
    )
}
