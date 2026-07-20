import { describe, it, expect } from 'vitest'
import { formatDuration, formatTime } from './tauri'

describe('formatDuration', () => {
    it('formats null/zero/negative as 0:00', () => {
        expect(formatDuration(null)).toBe('0:00')
        expect(formatDuration(0)).toBe('0:00')
        expect(formatDuration(-5)).toBe('0:00')
    })

    it('formats sub-minute durations as m:ss', () => {
        expect(formatDuration(5)).toBe('0:05')
        expect(formatDuration(59)).toBe('0:59')
    })

    it('formats minute-scale durations without a leading hour', () => {
        expect(formatDuration(65)).toBe('1:05')
        expect(formatDuration(600)).toBe('10:00')
    })

    it('formats hour-scale durations as h:mm:ss', () => {
        expect(formatDuration(3600)).toBe('1:00:00')
        expect(formatDuration(3665)).toBe('1:01:05')
        expect(formatDuration(7325)).toBe('2:02:05')
    })

    it('truncates fractional seconds rather than rounding', () => {
        expect(formatDuration(59.9)).toBe('0:59')
    })
})

describe('formatTime', () => {
    it('formats zero as 0:00 (no null-guard, unlike formatDuration)', () => {
        expect(formatTime(0)).toBe('0:00')
    })

    it('formats minute- and hour-scale positions the same as formatDuration', () => {
        expect(formatTime(65)).toBe('1:05')
        expect(formatTime(3665)).toBe('1:01:05')
    })
})
