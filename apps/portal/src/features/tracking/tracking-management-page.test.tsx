import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
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
    expect(await screen.findByText(/Belum ada target/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Device' }))
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
  })

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
})
