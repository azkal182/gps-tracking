import { describe, expect, it } from 'vitest'
import { getMapStyleUrl } from './map-config'

describe('map provider configuration', () => {
  it('prefers an explicitly configured style URL', () => {
    expect(
      getMapStyleUrl({
        VITE_MAP_STYLE_URL: 'https://tiles.example/style.json',
        VITE_MAPTILER_KEY: 'ignored',
      }),
    ).toBe('https://tiles.example/style.json')
  })

  it('builds the MapTiler streets style from the public browser key', () => {
    expect(getMapStyleUrl({ VITE_MAPTILER_KEY: 'public key' })).toBe(
      'https://api.maptiler.com/maps/streets-v2/style.json?key=public%20key',
    )
  })

  it('returns no provider URL when the operator has not configured one', () => {
    expect(getMapStyleUrl({})).toBeNull()
  })
})
