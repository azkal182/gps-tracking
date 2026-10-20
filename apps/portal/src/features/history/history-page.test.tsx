import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { App } from '@/App'

const tenantId = 'tenant-history-a'
const deviceId = 'device-history-a'
const sessionId = 'session-history-a'
const locationFixture = { latitude: -6.2, longitude: 106.8 }
const me = {
  user: {
    id: 'user-history',
    email: 'admin@example.invalid',
    display_name: 'Admin',
    status: 'active',
    created_at: '2026-10-20T00:00:00Z',
    updated_at: '2026-10-20T00:00:00Z',
  },
  mfa_required: true,
  mfa_enabled: true,
  memberships: [
    {
      id: 'membership-history',
      tenant_id: tenantId,
      user_id: 'user-history',
      role: 'admin',
      status: 'active',
      created_at: '2026-10-20T00:00:00Z',
      updated_at: '2026-10-20T00:00:00Z',
    },
  ],
}
const device = {
  id: deviceId,
  tenant_id: tenantId,
  name: 'Tracker off',
  protocol: 'http',
  identifier: 'tracker-off',
  ingestion_policy: 'session_only',
  metadata: {},
  enabled: false,
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
}
const session = {
  id: sessionId,
  tenant_id: tenantId,
  target_id: 'target-history-a',
  device_id: deviceId,
  status: 'ended',
  started_at: '2026-10-19T00:00:00Z',
  ended_at: '2026-10-19T04:00:00Z',
  created_at: '2026-10-19T00:00:00Z',
}
const position = {
  id: 'position-1',
  tenant_id: tenantId,
  device_id: deviceId,
  message_id: 'message-1',
  ...locationFixture,
  altitude_m: null,
  speed_mps: null,
  heading_deg: null,
  accuracy_m: 4,
  recorded_at: '2026-10-20T00:00:00Z',
  received_at: '2026-10-20T00:00:01Z',
  attributes: {},
}
const auditEvent = {
  id: 'audit-1',
  tenant_id: tenantId,
  actor_id: 'user-history',
  action: 'history.read',
  resource_type: 'device',
  resource_id: deviceId,
  outcome: 'success',
  request_id: 'request-1',
  occurred_at: '2026-10-20T00:00:02Z',
}

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}
function request(input: RequestInfo | URL) {
  return input instanceof Request ? input : new Request(input)
}

beforeEach(() => {
  window.sessionStorage.clear()
  window.localStorage.clear()
  vi.stubEnv('VITE_MAP_STYLE_URL', '')
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const current = request(input)
      const url = new URL(current.url)
      if (url.pathname.endsWith('/auth/me')) return json(me)
      if (url.pathname.endsWith('/organizations'))
        return json({
          data: [
            {
              id: tenantId,
              tenant_id: tenantId,
              name: 'History Org',
              status: 'active',
              created_at: '2026-10-20T00:00:00Z',
              updated_at: '2026-10-20T00:00:00Z',
            },
          ],
          next_cursor: null,
        })
      if (url.pathname.endsWith('/organization/audit-events'))
        return json({ data: [auditEvent], next_cursor: null })
      if (url.pathname.endsWith('/devices'))
        return json({ data: [device], next_cursor: null })
      if (url.pathname.endsWith('/sessions'))
        return json({ data: [session], next_cursor: null })
      if (
        url.pathname.endsWith('/devices/' + deviceId + '/positions') ||
        url.pathname.endsWith('/sessions/' + sessionId + '/positions')
      ) {
        if (url.searchParams.get('limit') === '1000')
          return json({ data: [position], next_cursor: null })
        if (url.searchParams.has('cursor'))
          return json({ data: [], next_cursor: null })
        return json({ data: [position], next_cursor: 'cursor-next' })
      }
      return json({})
    }),
  )
})
afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})

describe('history portal', () => {
  it('queries authorized device history using explicit UTC boundaries, opaque cursors, and keeps disabled-device history available', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(
      await screen.findByRole('link', { name: 'Peta & histori' }),
    )
    expect(
      await screen.findByRole('option', { name: 'Tracker off · nonaktif' }),
    ).toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText('Device'), deviceId)
    fireEvent.change(screen.getByLabelText('Dari (inklusif)'), {
      target: { value: '2026-10-19T00:00' },
    })
    fireEvent.change(screen.getByLabelText('Sampai (eksklusif)'), {
      target: { value: '2026-10-20T00:00' },
    })
    await user.click(screen.getByRole('button', { name: 'Terapkan rentang' }))
    expect(await screen.findByText('106.8')).toBeInTheDocument()
    expect(
      screen.getByRole('heading', { name: 'Audit organisasi' }),
    ).toBeInTheDocument()
    const positionRequests = vi
      .mocked(fetch)
      .mock.calls.map(([input]) => request(input))
      .filter((item) =>
        item.url.includes('/devices/' + deviceId + '/positions'),
      )
    const tableRequest = positionRequests
      .map((item) => new URL(item.url))
      .filter((url) => url.searchParams.get('limit') === '100')
      .at(-1)!
    expect(tableRequest.searchParams.get('from')).toBe(
      new Date(
        (screen.getByLabelText('Dari (inklusif)') as HTMLInputElement).value,
      ).toISOString(),
    )
    expect(tableRequest.searchParams.get('to')).toBe(
      new Date(
        (screen.getByLabelText('Sampai (eksklusif)') as HTMLInputElement).value,
      ).toISOString(),
    )
    expect(
      positionRequests.some(
        (item) => new URL(item.url).searchParams.get('limit') === '1000',
      ),
    ).toBe(true)
    expect(positionRequests.at(-1)?.headers.get('X-Organization-Id')).toBe(
      tenantId,
    )
    await user.click(screen.getAllByRole('button', { name: 'Berikutnya' })[0]!)
    await waitFor(() =>
      expect(
        vi
          .mocked(fetch)
          .mock.calls.some(
            ([input]) =>
              new URL(request(input).url).searchParams.get('cursor') ===
              'cursor-next',
          ),
      ).toBe(true),
    )
    expect(
      screen.getByText('Tidak ada posisi tersimpan dalam rentang ini.'),
    ).toBeInTheDocument()
    expect(window.localStorage.length).toBe(0)
    expect(
      [...Array(window.sessionStorage.length)]
        .map((_, index) =>
          window.sessionStorage.getItem(window.sessionStorage.key(index)!),
        )
        .join(' '),
    ).not.toContain(String(locationFixture.latitude))
    expect(screen.getByText('history.read')).toBeInTheDocument()
  })

  it('uses the session positions endpoint for a selected session', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(
      await screen.findByRole('link', { name: 'Peta & histori' }),
    )
    await user.selectOptions(screen.getByLabelText('Sumber histori'), 'session')
    await user.selectOptions(
      await screen.findByLabelText('Sesi tracking'),
      sessionId,
    )
    expect(await screen.findByText('106.8')).toBeInTheDocument()
    expect(
      vi
        .mocked(fetch)
        .mock.calls.some(([input]) =>
          request(input).url.includes('/sessions/' + sessionId + '/positions'),
        ),
    ).toBe(true)
  })

  it('does not render organization audit controls for non-admin membership', async () => {
    const viewerMe = {
      ...me,
      memberships: me.memberships.map((membership) => ({
        ...membership,
        role: 'viewer',
      })),
    }
    vi.mocked(fetch).mockImplementation(async (input) => {
      const current = request(input)
      const url = new URL(current.url)
      if (url.pathname.endsWith('/auth/me')) return json(viewerMe)
      if (url.pathname.endsWith('/organizations'))
        return json({
          data: [
            {
              id: tenantId,
              tenant_id: tenantId,
              name: 'History Org',
              status: 'active',
              created_at: '2026-10-20T00:00:00Z',
              updated_at: '2026-10-20T00:00:00Z',
            },
          ],
          next_cursor: null,
        })
      if (url.pathname.endsWith('/devices'))
        return json({ data: [device], next_cursor: null })
      if (url.pathname.endsWith('/organization/audit-events'))
        return json({ data: [auditEvent], next_cursor: null })
      return json({ data: [], next_cursor: null })
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(
      await screen.findByRole('link', { name: 'Peta & histori' }),
    )
    expect(
      await screen.findByText(
        'Audit organisasi hanya tersedia bagi admin organisasi.',
      ),
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('heading', { name: 'Audit organisasi' }),
    ).not.toBeInTheDocument()
    await waitFor(() =>
      expect(
        vi
          .mocked(fetch)
          .mock.calls.some(([input]) =>
            request(input).url.includes('/organization/audit-events'),
          ),
      ).toBe(false),
    )
  })

  it('shows a safe access-denied state without rendering denied position data', async () => {
    vi.mocked(fetch).mockImplementation(async (input) => {
      const current = request(input)
      const url = new URL(current.url)
      if (url.pathname.endsWith('/auth/me')) return json(me)
      if (url.pathname.endsWith('/organizations'))
        return json({ data: [], next_cursor: null })
      if (url.pathname.endsWith('/devices'))
        return json({ data: [device], next_cursor: null })
      if (url.pathname.includes('/positions'))
        return json(
          {
            error: {
              code: 'FORBIDDEN',
              message: 'Forbidden',
              request_id: 'synthetic',
            },
          },
          403,
        )
      if (url.pathname.endsWith('/organization/audit-events'))
        return json({ data: [], next_cursor: null })
      return json({ data: [], next_cursor: null })
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(
      await screen.findByRole('link', { name: 'Peta & histori' }),
    )
    await user.selectOptions(await screen.findByLabelText('Device'), deviceId)
    expect(
      await screen.findAllByText(
        'Akses histori tidak diberikan untuk akun ini.',
      ),
    ).toHaveLength(2)
    expect(
      screen.queryByText(String(locationFixture.latitude)),
    ).not.toBeInTheDocument()
  })
})
