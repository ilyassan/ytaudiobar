import { useEffect, useRef, useState } from 'react'

interface ScrollingTextProps {
    text: string
    className?: string
    speed?: number // pixels per second, default 50
}

export function ScrollingText({ text, className = '', speed = 50 }: ScrollingTextProps) {
    const containerRef = useRef<HTMLDivElement>(null)
    const textRef = useRef<HTMLSpanElement>(null)
    const [shouldScroll, setShouldScroll] = useState(false)
    const [duration, setDuration] = useState(10)

    useEffect(() => {
        const checkOverflow = () => {
            if (containerRef.current && textRef.current) {
                const containerWidth = containerRef.current.offsetWidth
                const textWidth = textRef.current.offsetWidth

                if (textWidth > containerWidth) {
                    setShouldScroll(true)
                    // Calculate duration based on text width and speed
                    const calculatedDuration = textWidth / speed
                    setDuration(calculatedDuration)
                } else {
                    setShouldScroll(false)
                }
            }
        }

        // Check on mount and when text changes
        checkOverflow()

        // Recheck on window resize
        window.addEventListener('resize', checkOverflow)
        return () => window.removeEventListener('resize', checkOverflow)
    }, [text, speed])

    return (
        <div
            ref={containerRef}
            className={`relative overflow-hidden whitespace-nowrap ${className}`}
        >
            {shouldScroll ? (
                <span
                    ref={textRef}
                    className="inline-block whitespace-nowrap"
                    style={{
                        animation: `marquee ${duration}s linear infinite`,
                    }}
                >
                    {text} {/* Duplicate text for seamless loop */}
                    <span className="inline-block ml-[30px]">{text}</span>
                </span>
            ) : (
                <span ref={textRef} className="truncate block">
                    {text}
                </span>
            )}
        </div>
    )
}
