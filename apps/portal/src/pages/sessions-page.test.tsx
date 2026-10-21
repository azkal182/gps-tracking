import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { App } from '@/App'

const createdAt = '2026-10-20T00:00:00Z'

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

beforeEach(() => {
  window.sessionStorage.clear()
  window.localStorage.clear()
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const request = input instanceof Request ? input : new Request(input)
      const pathname = new URL(request.url).pathname
      if (pathname.endsWith('/auth/me'))
        return json({
          user: {
            id: 'session-user',
            email: 'user@example.invalid',
            display_name: 'User',
            status: 'active',
            created_at: createdAt,
            updated_at: createdAt,
          },
          platform_admin: false,
          mfa_required: false,
          mfa_enabled: false,
          memberships: [],
        })
      if (pathname.endsWith('/auth/csrf'))
        return json({ csrf_token: 'csrf-test' })
      if (pathname.endsWith('/organizations'))
        return json({ data: [], next_cursor: null })
      if (pathname.endsWith('/auth/sessions'))
        return json({
          data: [
            {
              id: 'session-other',
              created_at: createdAt,
              last_used_at: createdAt,
              expires_at: '2026-10-20T12:00:00Z',
              current: false,
              device_label: 'Browser lain',
            },
          ],
          next_cursor: null,
        })
      if (request.method === 'DELETE')
        return json({ error: { message: 'Pencabutan ditolak server' } }, 500)
      return json({})
    }),
  )
})

afterEach(() => vi.unstubAllGlobals())

describe('login session controls', () => {
  it('shows an error when session revocation fails', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Sesi login' }))
    await user.click(await screen.findByRole('button', { name: 'Cabut sesi' }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Pencabutan ditolak server',
    )
  })
})
