import { describe, expect, it } from 'vitest'
import {
  defaultHistoryRange,
  HISTORY_MAP_POINT_LIMIT,
  validateHistoryRange,
  visibleMapPositions,
} from './history-utils'

describe('history range and map limits', () => {
  it('defaults to the previous 24 hours and serializes local input as UTC', () => {
    const range = defaultHistoryRange(new Date('2026-10-20T12:00:00Z'))
    const parsed = validateHistoryRange(range)
    expect(parsed.value).toEqual({
      from: new Date(range.from).toISOString(),
      to: new Date(range.to).toISOString(),
    })
    expect(Date.parse(parsed.value!.to) - Date.parse(parsed.value!.from)).toBe(
      24 * 60 * 60 * 1000,
    )
  })

  it('accepts exactly 31 days but rejects a larger range and reversed boundaries', () => {
    const from = '2026-09-01T00:00'
    expect(
      validateHistoryRange({ from, to: '2026-10-02T00:00' }).value,
    ).toBeDefined()
    expect(
      validateHistoryRange({ from, to: '2026-10-02T00:00:01' }).error,
    ).toMatch(/31 hari/)
    expect(validateHistoryRange({ from, to: from }).error).toMatch(/setelah/)
  })

  it('caps map rendering at 10,000 without changing table data', () => {
    const rows = Array.from(
      { length: HISTORY_MAP_POINT_LIMIT + 25 },
      (_, index) => index,
    )
    expect(visibleMapPositions(rows)).toHaveLength(HISTORY_MAP_POINT_LIMIT)
    expect(rows).toHaveLength(HISTORY_MAP_POINT_LIMIT + 25)
  })
})
