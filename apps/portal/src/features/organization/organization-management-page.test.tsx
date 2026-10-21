import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { App } from '@/App'

const organization = {
  id: 'tenant-org-1',
  tenant_id: 'tenant-org-1',
  name: 'Operations',
  status: 'active',
  created_at: '2026-10-20T00:00:00Z',
  updated_at: '2026-10-20T00:00:00Z',
}
function json(data: unknown) {
  return new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}
function urlOf(input: RequestInfo | URL) {
  return input instanceof Request ? input.url : String(input)
}

beforeEach(() => {
  window.sessionStorage.clear()
  window.localStorage.clear()
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = urlOf(input)
      if (url.includes('/auth/me'))
        return json({
          user: {
            id: 'platform-user',
            email: 'platform@example.invalid',
            display_name: 'Platform Admin',
            status: 'active',
            created_at: organization.created_at,
            updated_at: organization.updated_at,
          },
          platform_admin: true,
          mfa_required: true,
          mfa_enabled: true,
          memberships: [],
        })
      if (url.includes('/platform/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.includes('/organizations'))
        return json({ data: [], next_cursor: null })
      return json({})
    }),
  )
})
afterEach(() => vi.unstubAllGlobals())

describe('platform organization management', () => {
  it('shows platform organization controls only for a platform admin profile', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(
      await screen.findByRole('link', { name: 'Organisasi platform' }),
    )
    expect(
      await screen.findByRole('heading', { name: 'Semua organisasi' }),
    ).toBeInTheDocument()
    expect(await screen.findByText('Operations')).toBeInTheDocument()
    expect(
      screen.getByRole('heading', { name: 'Buat organisasi' }),
    ).toBeInTheDocument()
    expect(
      screen.getByText(/tidak menampilkan lokasi atau histori/),
    ).toBeInTheDocument()
  })

  it('requires recent authentication before changing a member role', async () => {
    const fetchMock = vi.mocked(fetch)
    fetchMock.mockImplementation(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = urlOf(input)
        const method =
          input instanceof Request ? input.method : (init?.method ?? 'GET')
        if (url.includes('/auth/me'))
          return json({
            user: {
              id: 'org-admin',
              email: 'admin@example.invalid',
              display_name: 'Admin',
              status: 'active',
              created_at: organization.created_at,
              updated_at: organization.updated_at,
            },
            platform_admin: false,
            mfa_required: true,
            mfa_enabled: true,
            memberships: [
              {
                id: 'm-admin',
                tenant_id: organization.id,
                user_id: 'org-admin',
                role: 'admin',
                status: 'active',
                created_at: organization.created_at,
                updated_at: organization.updated_at,
              },
            ],
          })
        if (url.endsWith('/auth/csrf')) return json({ csrf_token: 'csrf-test' })
        if (url.includes('/organizations'))
          return json({ data: [organization], next_cursor: null })
        if (url.endsWith('/organization')) return json(organization)
        if (url.includes('/organization/members') && method === 'GET')
          return json({
            data: [
              {
                id: 'm-member',
                tenant_id: organization.id,
                user_id: 'user-member',
                role: 'operator',
                status: 'active',
                user: {
                  email: 'operator@example.invalid',
                  display_name: 'Operator',
                },
              },
            ],
            next_cursor: null,
          })
        if (url.includes('/organization/invitations'))
          return json({ data: [], next_cursor: null })
        if (url.includes('/auth/reauthenticate'))
          return new Response(null, { status: 204 })
        return json({})
      },
    )
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Organisasi' }))
    await user.click(
      await screen.findByRole('button', { name: 'Ubah peran/status' }),
    )
    await user.selectOptions(await screen.findByLabelText('Peran'), 'viewer')
    await user.click(screen.getByRole('button', { name: 'Simpan' }))
    await user.type(
      await screen.findByLabelText('Password saat ini'),
      'synthetic long password',
    )
    await user.click(screen.getByRole('button', { name: 'Konfirmasi' }))
    await waitFor(() =>
      expect(
        fetchMock.mock.calls.some(([input, init]) => {
          const url = urlOf(input)
          const method = input instanceof Request ? input.method : init?.method
          return (
            url.includes('/organization/members/user-member') &&
            method === 'PATCH'
          )
        }),
      ).toBe(true),
    )
  })

  it('replaces explicit grants after recent authentication', async () => {
    let replaced: { grants: unknown[] } | null = null
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const url = urlOf(input)
      const method =
        input instanceof Request ? input.method : (init?.method ?? 'GET')
      if (url.includes('/auth/me'))
        return json({
          user: {
            id: 'org-admin',
            email: 'admin@example.invalid',
            display_name: 'Admin',
            status: 'active',
            created_at: organization.created_at,
            updated_at: organization.updated_at,
          },
          platform_admin: false,
          mfa_required: true,
          mfa_enabled: true,
          memberships: [
            {
              id: 'm-admin',
              tenant_id: organization.id,
              user_id: 'org-admin',
              role: 'admin',
              status: 'active',
              created_at: organization.created_at,
              updated_at: organization.updated_at,
            },
          ],
        })
      if (url.includes('/auth/csrf')) return json({ csrf_token: 'csrf-test' })
      if (url.includes('/organizations'))
        return json({ data: [organization], next_cursor: null })
      if (url.endsWith('/organization')) return json(organization)
      if (
        url.includes('/organization/members/user-member/grants') &&
        method === 'GET'
      )
        return json({ grants: [] })
      if (url.includes('/organization/members') && method === 'GET')
        return json({
          data: [
            {
              id: 'm-member',
              tenant_id: organization.id,
              user_id: 'user-member',
              role: 'operator',
              status: 'active',
              user: {
                email: 'operator@example.invalid',
                display_name: 'Operator',
              },
            },
          ],
          next_cursor: null,
        })
      if (url.includes('/organization/invitations'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/targets?') || url.endsWith('/targets'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/devices?') || url.endsWith('/devices'))
        return json({ data: [], next_cursor: null })
      if (url.includes('/auth/reauthenticate'))
        return new Response(null, { status: 204 })
      if (
        url.includes('/organization/members/user-member/grants') &&
        method === 'PUT'
      ) {
        const body =
          input instanceof Request
            ? await input.clone().json()
            : JSON.parse(String(init?.body))
        replaced = body as { grants: unknown[] }
        return json({ grants: replaced.grants })
      }
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Organisasi' }))
    await user.click(await screen.findByRole('button', { name: 'Atur grants' }))
    await user.click(await screen.findByLabelText('Buat target'))
    await user.click(screen.getByRole('button', { name: 'Simpan grants' }))
    await user.type(
      await screen.findByLabelText('Password saat ini'),
      'synthetic long password',
    )
    await user.click(screen.getByRole('button', { name: 'Konfirmasi' }))
    await waitFor(() =>
      expect(replaced?.grants).toEqual([
        {
          resource_type: 'organization',
          resource_id: organization.id,
          permissions: ['targets:create'],
        },
      ]),
    )
  }, 20_000)

  it('keeps the platform organization UI unavailable to a normal account', async () => {
    vi.mocked(fetch).mockImplementation(async (input: RequestInfo | URL) => {
      const url = urlOf(input)
      if (url.includes('/auth/me'))
        return json({
          user: {
            id: 'tenant-user',
            email: 'user@example.invalid',
            display_name: 'User',
            status: 'active',
            created_at: organization.created_at,
            updated_at: organization.updated_at,
          },
          platform_admin: false,
          mfa_required: false,
          mfa_enabled: false,
          memberships: [],
        })
      if (url.includes('/organizations'))
        return json({ data: [], next_cursor: null })
      return json({})
    })
    window.history.pushState({}, '', '/platform/organizations')
    render(<App />)
    expect(
      await screen.findByText(/hanya tersedia untuk pengelola platform/),
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('heading', { name: 'Buat organisasi' }),
    ).not.toBeInTheDocument()
    expect(
      vi
        .mocked(fetch)
        .mock.calls.some(([input]) =>
          urlOf(input).includes('/platform/organizations'),
        ),
    ).toBe(false)
  })

  it('switches the member list and request context with the active tenant', async () => {
    const second = {
      ...organization,
      id: 'tenant-org-2',
      tenant_id: 'tenant-org-2',
      name: 'Dispatch',
    }
    vi.mocked(fetch).mockImplementation(async (input) => {
      const request = input instanceof Request ? input : new Request(input)
      const url = request.url
      const tenantId = request.headers.get('X-Organization-Id')
      if (url.includes('/auth/me'))
        return json({
          user: {
            id: 'multi-admin',
            email: 'admin@example.invalid',
            display_name: 'Admin',
            status: 'active',
            created_at: organization.created_at,
            updated_at: organization.updated_at,
          },
          platform_admin: false,
          mfa_required: true,
          mfa_enabled: true,
          memberships: [organization, second].map((org, index) => ({
            id: `m-admin-${index}`,
            tenant_id: org.id,
            user_id: 'multi-admin',
            role: 'admin',
            status: 'active',
            created_at: organization.created_at,
            updated_at: organization.updated_at,
          })),
        })
      if (url.includes('/organizations'))
        return json({ data: [organization, second], next_cursor: null })
      if (url.includes('/organization/members?'))
        return json({
          data: [
            {
              id: `member-${tenantId}`,
              tenant_id: tenantId,
              user_id: `user-${tenantId}`,
              role: 'operator',
              status: 'active',
              user: {
                email: `${tenantId}@example.invalid`,
                display_name:
                  tenantId === second.id
                    ? 'Dispatch member'
                    : 'Operations member',
              },
            },
          ],
          next_cursor: null,
        })
      if (url.endsWith('/organization'))
        return json(tenantId === second.id ? second : organization)
      if (url.includes('/organization/invitations'))
        return json({ data: [], next_cursor: null })
      return json({})
    })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('link', { name: 'Organisasi' }))
    expect(await screen.findByText('Operations member')).toBeInTheDocument()
    await user.selectOptions(
      screen.getAllByLabelText('Organisasi aktif')[1],
      second.id,
    )
    expect(await screen.findByText('Dispatch member')).toBeInTheDocument()
    expect(screen.queryByText('Operations member')).not.toBeInTheDocument()
  }, 20_000)
})
