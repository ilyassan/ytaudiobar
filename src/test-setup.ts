import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'

// Without this, a component/hook rendered in one test (and its window-level
// event listeners, in useKeyboardShortcuts' case) stays mounted into the
// next test and causes cross-test leakage/duplicate-call bugs.
afterEach(() => {
    cleanup()
})
