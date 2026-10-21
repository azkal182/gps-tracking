import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { App } from '@/App'

const tenant = 'tenant-portal-1'
const organization = {
  id: tenant,
  tenant_id: tenant,
  name: 'Operations',
  status: 'active',
  created_at: '2026-10-19T00:00:00Z',
  updated_at: '2026-10-19T00:00:00Z',
}
const membership = (role: 'admin' | 'operator' | 'viewer' = 'admin') => ({
  id: 'membership-1',
  tenant_id: tenant,
  user_id: 'user-1',
  role,
  status: 'active',
  created_at: organization.created_at,
  updated_at: organization.updated_at,
})
const userData = (role: 'admin' | 'operator' | 'viewer' = 'admin') => ({
  user: {
    id: 'user-1',
    email: 'admin@example.invalid',
    display_name: 'Portal Admin',
    status: 'active',
    created_at: organization.created_at,
    updated_at: organization.updated_at,
  },
  mfa_required: true,
  mfa_enabled: true,
  memberships: [membership(role)],
})
const deviceId = 'd19d3f58-979a-42ad-9e3d-6e453c265fbd'
const device = {
  id: deviceId,
  tenant_id: tenant,
  name: 'Van 01',
  protocol: 'http-json',
  identifier: 'vehicle-01',
  ingestion_policy: 'session_only',
  metadata: {},
  enabled: true,
  created_at: organization.created_at,
  updated_at: organization.updated_at,
}
let registeredDevice = false
function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}
function urlOf(input: RequestInfo | URL) {
  return input instanceof Request ? input.url : String(input)
}

beforeEach(() => {
  registeredDevice = false
  window.sessionStorage.clear()
  window.localStorage.clear()
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = urlOf(input)
      const method =
        (input instanceof Request ? input.method : init?.method) ?? 'GET'
      if (url.includes('/auth/me')) return json(userData())
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets?') || url.endsWith('/targets'))
        return json({ data: [], next_cursor: null })
      if (url.endsWith('/devices') && method === 'POST') {
        registeredDevice = true
        return json(device, 201)
      }
      if (url.includes('/devices?') || url.endsWith('/devices'))
        return json({
          data: registeredDevice ? [device] : [],
          next_cursor: null,
        })
      if (url.includes('/sessions?') || url.endsWith('/sessions'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/members/user-1/grants')) return json({ grants: [] })
      if (url.includes('/auth/csrf'))
        return json({ csrf_token: 'csrf-synthetic' })
      if (url.includes('/auth/reauthenticate') && method === 'POST')
        return new Response(null, { status: 204 })
      if (url.includes('/credentials/rotate'))
        return json({
          device_id: deviceId,
          token: 'one-time-synthetic-device-token',
          created_at: organization.created_at,
        })
      return json({})
    }),
  )
})
afterEach(() => vi.unstubAllGlobals())

describe('tracking management portal', () => {
  it('registers without issuing a token, then shows a rotated token once and clears it', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    await user.click(await screen.findByRole('button', { name: 'Device' }))
    await user.type(screen.getByLabelText('Nama'), 'Van 01')
    await user.type(screen.getByLabelText('Identifier'), 'vehicle-01')
    await user.click(screen.getByRole('button', { name: 'Simpan' }))
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Device terdaftar tanpa token',
    )
    expect(
      screen.queryByText('one-time-synthetic-device-token'),
    ).not.toBeInTheDocument()

    await user.click(
      await screen.findByRole('button', { name: /Terbitkan \/ rotasi token/ }),
    )
    await user.type(
      screen.getByLabelText('Password saat ini'),
      'a synthetic long password',
    )
    await user.click(
      screen.getByRole('button', { name: 'Konfirmasi dan rotasi' }),
    )
    expect(
      await screen.findByText('one-time-synthetic-device-token'),
    ).toBeInTheDocument()
    expect(window.localStorage.length).toBe(0)
    await user.click(
      screen.getByRole('button', { name: 'Tutup dan hapus token' }),
    )
    await waitFor(() =>
      expect(
        screen.queryByText('one-time-synthetic-device-token'),
      ).not.toBeInTheDocument(),
    )
  }, 20_000)

  it('does not render write actions for a viewer and still explains the empty state', async () => {
    vi.mocked(fetch).mockImplementation(async (input) => {
      const url = urlOf(input)
      if (url.includes('/auth/me')) return json(userData('viewer'))
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets?') || url.endsWith('/targets'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/devices?') || url.endsWith('/devices'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/sessions?') || url.endsWith('/sessions'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/members/user-1/grants')) return json({ grants: [] })
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    expect(
      await screen.findByText(/Belum ada device terdaftar/),
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: 'Device' }),
    ).not.toBeInTheDocument()
    expect(
      screen.getByText(/memerlukan role\/grant yang sesuai/),
    ).toBeInTheDocument()
  })

  it('explains active-session conflicts returned by the API', async () => {
    const target = {
      id: '7c5db7cc-ea93-4fd7-b0f4-27e468e6a212',
      name: 'Route A',
      external_ref: null,
    }
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const url = urlOf(input)
      const method =
        (input instanceof Request ? input.method : init?.method) ?? 'GET'
      if (url.includes('/auth/me')) return json(userData())
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets?'))
        return json({ data: [target], next_cursor: null })
      if (url.includes('/devices?'))
        return json({ data: [device], next_cursor: null })
      if (url.includes('/sessions?'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/sessions') && method === 'POST')
        return json(
          {
            error: {
              code: 'SESSION_CONFLICT',
              message: 'Device sudah memiliki tracking session aktif.',
              request_id: 'synthetic-request',
            },
          },
          409,
        )
      if (url.includes('/auth/csrf'))
        return json({ csrf_token: 'csrf-synthetic' })
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    await user.click(
      await screen.findByRole('button', { name: /Mulai session/ }),
    )
    await user.selectOptions(screen.getByLabelText('Target'), target.id)
    await user.selectOptions(screen.getByLabelText('Device aktif'), device.id)
    await user.click(screen.getByRole('button', { name: 'Simpan' }))
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Device sudah memiliki tracking session aktif.',
    )
  })

  it('shows operator actions only for the device grants returned by the API', async () => {
    vi.mocked(fetch).mockImplementation(async (input) => {
      const url = urlOf(input)
      if (url.includes('/auth/me')) return json(userData('operator'))
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets?'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/devices?'))
        return json({ data: [device], next_cursor: null })
      if (url.includes('/sessions?'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/members/user-1/grants'))
        return json({
          grants: [
            {
              resource_type: 'device',
              resource_id: deviceId,
              permissions: ['read', 'credentials:write'],
            },
          ],
        })
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    expect(
      await screen.findByRole('button', { name: /Terbitkan \/ rotasi token/ }),
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: 'Aktifkan' }),
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: 'Device' }),
    ).not.toBeInTheDocument()
  })

  it('edits mutable device fields and leaves protocol and identifier immutable', async () => {
    let patchBody: Record<string, unknown> | null = null
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const url = urlOf(input)
      const method =
        input instanceof Request ? input.method : (init?.method ?? 'GET')
      if (url.includes('/auth/me')) return json(userData())
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets')) return json({ data: [], next_cursor: null })
      if (
        (url.includes('/devices?') || url.endsWith('/devices')) &&
        method === 'GET'
      )
        return json({ data: [device], next_cursor: null })
      if (url.includes(`/devices/${deviceId}`) && method === 'PATCH') {
        const body =
          input instanceof Request
            ? await input.clone().json()
            : JSON.parse(String(init?.body))
        patchBody = body as Record<string, unknown>
        return json(device)
      }
      if (url.includes('/sessions'))
        return json({ data: [], next_cursor: null })
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    await user.click(await screen.findByRole('button', { name: 'Edit detail' }))
    const name = await screen.findByLabelText('Nama')
    await user.clear(name)
    await user.type(name, 'Van updated')
    await user.clear(screen.getByLabelText('Metadata (JSON object)'))
    fireEvent.change(screen.getByLabelText('Metadata (JSON object)'), {
      target: { value: '{"team":"north"}' },
    })
    await user.selectOptions(
      screen.getByLabelText('Kebijakan ingestion'),
      'always',
    )
    await user.click(screen.getByRole('button', { name: 'Simpan' }))
    await waitFor(() =>
      expect(patchBody).toEqual({
        name: 'Van updated',
        ingestion_policy: 'always',
        metadata: { team: 'north' },
      }),
    )
    expect(patchBody).not.toHaveProperty('protocol')
    expect(patchBody).not.toHaveProperty('identifier')
  })

  it('lists credential metadata only and requires recent authentication to revoke one', async () => {
    let revoked = false
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const url = urlOf(input)
      const method =
        input instanceof Request ? input.method : (init?.method ?? 'GET')
      if (url.includes('/auth/me')) return json(userData())
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets')) return json({ data: [], next_cursor: null })
      if (url.includes('/devices?'))
        return json({ data: [device], next_cursor: null })
      if (url.includes(`/devices/${deviceId}/credentials?`))
        return json({
          data: [
            {
              id: 'credential-1',
              created_at: organization.created_at,
              revoked_at: revoked ? organization.updated_at : null,
              active: !revoked,
              token: 'must-not-display',
            },
          ],
          next_cursor: null,
        })
      if (url.includes('/auth/csrf'))
        return json({ csrf_token: 'csrf-synthetic' })
      if (url.includes('/auth/reauthenticate') && method === 'POST')
        return new Response(null, { status: 204 })
      if (
        url.includes(`/devices/${deviceId}/credentials/credential-1`) &&
        method === 'DELETE'
      ) {
        revoked = true
        return new Response(null, { status: 204 })
      }
      if (url.includes('/sessions'))
        return json({ data: [], next_cursor: null })
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    await user.click(await screen.findByRole('button', { name: 'Credentials' }))
    expect(await screen.findByText(/Aktif · dibuat/)).toBeInTheDocument()
    expect(screen.queryByText('must-not-display')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Cabut credential' }))
    await user.click(screen.getByRole('button', { name: /^Konfirmasi$/ }))
    await user.type(
      await screen.findByLabelText('Password saat ini'),
      'synthetic long password',
    )
    await user.click(
      screen.getByRole('button', { name: 'Konfirmasi dan cabut' }),
    )
    await waitFor(() => expect(revoked).toBe(true))
    expect(await screen.findByText(/Dicabut · dibuat/)).toBeInTheDocument()
  }, 20_000)

  it('confirms device disable before revoking its credential access', async () => {
    let patchBody: Record<string, unknown> | null = null
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const url = urlOf(input)
      const method =
        input instanceof Request ? input.method : (init?.method ?? 'GET')
      if (url.includes('/auth/me')) return json(userData())
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets')) return json({ data: [], next_cursor: null })
      if (url.includes('/devices?'))
        return json({ data: [device], next_cursor: null })
      if (url.includes('/sessions'))
        return json({ data: [], next_cursor: null })
      if (url.includes(`/devices/${deviceId}`) && method === 'PATCH') {
        const body =
          input instanceof Request
            ? await input.clone().json()
            : JSON.parse(String(init?.body))
        patchBody = body as Record<string, unknown>
        return json({ ...device, enabled: false })
      }
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    await user.click(await screen.findByRole('button', { name: 'Nonaktifkan' }))
    expect(screen.getByText(/Credential aktif dicabut/)).toBeInTheDocument()
    expect(patchBody).toBeNull()
    await user.click(screen.getByRole('button', { name: /^Konfirmasi$/ }))
    await waitFor(() => expect(patchBody).toEqual({ enabled: false }))
  }, 20_000)

  it('filters targets through the API, follows opaque cursors, and opens tenant-scoped details', async () => {
    const target = {
      id: '7c5db7cc-ea93-4fd7-b0f4-27e468e6a212',
      tenant_id: tenant,
      name: 'Route A',
      external_ref: 'route-a',
      metadata: { team: 'north' },
      created_at: organization.created_at,
      updated_at: organization.updated_at,
    }
    vi.mocked(fetch).mockImplementation(async (input) => {
      const url = urlOf(input)
      if (url.includes('/auth/me')) return json(userData())
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/targets?'))
        return json({
          data: [target],
          next_cursor: new URL(url).searchParams.has('cursor')
            ? null
            : 'opaque-target-cursor',
        })
      if (url.endsWith(`/targets/${target.id}`)) return json(target)
      if (url.includes('/devices?'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/sessions?'))
        return json({ data: [], next_cursor: null })
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Devices' }))
    await user.type(await screen.findByLabelText('Cari target'), 'Route')
    await waitFor(() =>
      expect(
        vi
          .mocked(fetch)
          .mock.calls.some(
            ([input]) =>
              new URL(urlOf(input)).searchParams.get('q') === 'Route',
          ),
      ).toBe(true),
    )
    await user.click(screen.getAllByRole('button', { name: 'Berikutnya' })[0])
    await waitFor(() =>
      expect(
        vi
          .mocked(fetch)
          .mock.calls.some(
            ([input]) =>
              new URL(urlOf(input)).searchParams.get('cursor') ===
              'opaque-target-cursor',
          ),
      ).toBe(true),
    )
    await user.click(
      await screen.findByRole('button', { name: 'Detail target' }),
    )
    expect(await screen.findByText(/Metadata:/)).toBeInTheDocument()
    expect(screen.getByText(/"team":"north"/)).toBeInTheDocument()
  }, 20_000)
})
