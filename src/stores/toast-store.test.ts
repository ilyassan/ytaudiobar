import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { useToastStore } from './toast-store'

describe('toast-store', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        useToastStore.setState({ toasts: [] })
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('starts with an empty toast queue', () => {
        expect(useToastStore.getState().toasts).toEqual([])
    })

    it('show() appends a toast with the given message and variant', () => {
        useToastStore.getState().show('Something failed', 'error')

        const { toasts } = useToastStore.getState()
        expect(toasts).toHaveLength(1)
        expect(toasts[0].message).toBe('Something failed')
        expect(toasts[0].variant).toBe('error')
    })

    it('show() defaults to the error variant when none is given', () => {
        useToastStore.getState().show('Default variant')
        expect(useToastStore.getState().toasts[0].variant).toBe('error')
    })

    it('multiple show() calls queue up distinct toasts with distinct ids', () => {
        useToastStore.getState().show('First')
        useToastStore.getState().show('Second')

        const { toasts } = useToastStore.getState()
        expect(toasts).toHaveLength(2)
        expect(toasts[0].id).not.toBe(toasts[1].id)
    })

    it('dismiss() removes only the toast with the matching id', () => {
        useToastStore.getState().show('Keep me')
        useToastStore.getState().show('Remove me')
        const [keep, remove] = useToastStore.getState().toasts

        useToastStore.getState().dismiss(remove.id)

        const { toasts } = useToastStore.getState()
        expect(toasts).toHaveLength(1)
        expect(toasts[0].id).toBe(keep.id)
    })

    it('dismiss() with an unknown id is a harmless no-op', () => {
        useToastStore.getState().show('Still here')
        useToastStore.getState().dismiss(999999)
        expect(useToastStore.getState().toasts).toHaveLength(1)
    })

    it('a toast auto-dismisses after the timeout elapses', () => {
        useToastStore.getState().show('Temporary')
        expect(useToastStore.getState().toasts).toHaveLength(1)

        vi.advanceTimersByTime(4000)

        expect(useToastStore.getState().toasts).toHaveLength(0)
    })

    it('a toast does not dismiss before the timeout elapses', () => {
        useToastStore.getState().show('Not yet')
        vi.advanceTimersByTime(3999)
        expect(useToastStore.getState().toasts).toHaveLength(1)
    })
})
