import { expect, test } from '@playwright/test'

const tenantId = 'tenant-browser-history'
const deviceId = 'device-browser-history'
const sessionId = 'session-browser-history'
const position = {
  id: '101',
  tenant_id: tenantId,
  device_id: deviceId,
  message_id: 'browser-fixture',
  latitude: -6.2,
  longitude: 106.8,
  altitude_m: null,
  speed_mps: null,
  heading_deg: null,
  accuracy_m: 4,
  recorded_at: '2026-10-20T00:00:00Z',
  received_at: '2026-10-20T00:00:01Z',
  attributes: {},
}
const session = {
  id: sessionId,
  tenant_id: tenantId,
  target_id: 'target-browser-history',
  device_id: deviceId,
  status: 'ended',
  started_at: '2026-10-19T00:00:00Z',
  ended_at: '2026-10-19T04:00:00Z',
  created_at: '2026-10-19T00:00:00Z',
}

test('admin reads device/session history with UTC filters, cursor paging and audit', async ({
  page,
}) => {
  const observedRequests: URL[] = []
  await page.route('**/v1/**', async (route) => {
    const url = new URL(route.request().url())
    observedRequests.push(url)
    let body: unknown = {}
    if (url.pathname.endsWith('/auth/me')) {
      body = {
        user: {
          id: 'user-browser',
          email: 'admin@example.invalid',
          display_name: 'Admin',
          status: 'active',
          created_at: '2026-10-19T00:00:00Z',
          updated_at: '2026-10-19T00:00:00Z',
        },
        mfa_required: true,
        mfa_enabled: true,
        memberships: [
          {
            id: 'membership-browser',
            tenant_id: tenantId,
            user_id: 'user-browser',
            role: 'admin',
            status: 'active',
            created_at: '2026-10-19T00:00:00Z',
            updated_at: '2026-10-19T00:00:00Z',
          },
        ],
      }
    } else if (url.pathname.endsWith('/organizations')) {
      body = {
        data: [
          {
            id: tenantId,
            tenant_id: tenantId,
            name: 'Browser Org',
            status: 'active',
            created_at: '2026-10-19T00:00:00Z',
            updated_at: '2026-10-19T00:00:00Z',
          },
        ],
        next_cursor: null,
      }
    } else if (url.pathname.endsWith('/devices')) {
      body = {
        data: [
          {
            id: deviceId,
            tenant_id: tenantId,
            name: 'Disabled tracker',
            protocol: 'http',
            identifier: 'disabled-tracker',
            ingestion_policy: 'session_only',
            metadata: {},
            enabled: false,
            created_at: '2026-10-19T00:00:00Z',
            updated_at: '2026-10-19T00:00:00Z',
          },
        ],
        next_cursor: null,
      }
    } else if (url.pathname.endsWith('/sessions')) {
      body = { data: [session], next_cursor: null }
    } else if (url.pathname.endsWith('/organization/audit-events')) {
      body = {
        data: [
          {
            id: 'audit-browser',
            tenant_id: tenantId,
            actor_id: 'user-browser',
            action: 'history.read',
            resource_type: 'device',
            resource_id: deviceId,
            outcome: 'success',
            request_id: 'request-browser',
            occurred_at: '2026-10-20T00:00:02Z',
          },
        ],
        next_cursor: null,
      }
    } else if (url.pathname.endsWith('/positions')) {
      body =
        url.searchParams.get('limit') === '1000'
          ? { data: [position], next_cursor: null }
          : url.searchParams.has('cursor')
            ? { data: [], next_cursor: null }
            : { data: [position], next_cursor: 'opaque-browser-cursor' }
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(body),
    })
  })

  await page.goto('/history')
  await expect(
    page.getByRole('heading', { name: 'Peta dan histori' }),
  ).toBeVisible()
  await page.getByRole('combobox', { name: 'Device' }).selectOption(deviceId)
  await page
    .getByRole('textbox', { name: 'Dari (inklusif)' })
    .fill('2026-10-19T00:00')
  await page
    .getByRole('textbox', { name: 'Sampai (eksklusif)' })
    .fill('2026-10-20T00:00')
  await page.getByRole('button', { name: 'Terapkan rentang' }).click()
  await expect(page.getByText('106.8', { exact: true })).toBeVisible()
  await expect(
    page.getByRole('heading', { name: 'Audit organisasi' }),
  ).toBeVisible()
  await expect(page.getByText('history.read')).toBeVisible()
  const deviceRequest = observedRequests
    .filter(
      (url) =>
        url.pathname.endsWith(`/devices/${deviceId}/positions`) &&
        url.searchParams.get('limit') === '100',
    )
    .at(-1)
  const fromInput = page.getByRole('textbox', { name: 'Dari (inklusif)' })
  const toInput = page.getByRole('textbox', { name: 'Sampai (eksklusif)' })
  expect(await fromInput.inputValue()).toBe('2026-10-19T00:00')
  expect(await toInput.inputValue()).toBe('2026-10-20T00:00')
  expect(deviceRequest?.searchParams.get('from')).toBe(
    await fromInput.evaluate((input: HTMLInputElement) =>
      new Date(input.value).toISOString(),
    ),
  )
  expect(deviceRequest?.searchParams.get('to')).toBe(
    await toInput.evaluate((input: HTMLInputElement) =>
      new Date(input.value).toISOString(),
    ),
  )

  await page.getByRole('button', { name: 'Berikutnya' }).first().click()
  await expect(
    page.getByText('Tidak ada posisi tersimpan dalam rentang ini.'),
  ).toBeVisible()
  await expect
    .poll(() =>
      observedRequests.some(
        (url) => url.searchParams.get('cursor') === 'opaque-browser-cursor',
      ),
    )
    .toBe(true)

  await page
    .getByRole('combobox', { name: 'Sumber histori' })
    .selectOption('session')
  await page
    .getByRole('combobox', { name: 'Sesi tracking' })
    .selectOption(sessionId)
  await expect(page.getByText('106.8', { exact: true })).toBeVisible()
  expect(
    observedRequests.some((url) =>
      url.pathname.endsWith(`/sessions/${sessionId}/positions`),
    ),
  ).toBe(true)
  expect(
    await page.evaluate(() =>
      JSON.stringify({ local: localStorage, session: sessionStorage }),
    ),
  ).not.toContain(String(position.latitude))
})
