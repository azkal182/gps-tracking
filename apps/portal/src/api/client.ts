import createClient from 'openapi-fetch'
import type { Middleware } from 'openapi-fetch'
import type { paths } from './generated'

export type PortalApiContext = {
  organizationId?: () => string | null
  csrfToken?: () => string | null
}

let csrfToken: string | null = null
const tenantStorageKey = 'gps.active-organization'

export function createTabOrganizationStore(
  storage: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>,
) {
  return {
    get: () => storage.getItem(tenantStorageKey),
    set: (id: string | null) =>
      id
        ? storage.setItem(tenantStorageKey, id)
        : storage.removeItem(tenantStorageKey),
  }
}

const tabOrganizationStore =
  typeof window === 'undefined'
    ? null
    : createTabOrganizationStore(window.sessionStorage)

export function setPortalCsrfToken(token: string | null) {
  csrfToken = token
}

export function getPortalCsrfToken() {
  return csrfToken
}

async function refreshPortalCsrf() {
  const { data, error } = await portalApi.GET('/auth/csrf')
  if (error || !data) throw error ?? new Error('CSRF token tidak tersedia')
  csrfToken = data.csrf_token
}

/** Serialize CSRF rotation and the mutation across same-origin tabs sharing one session cookie. */
export async function withPortalMutation<T>(
  operation: () => Promise<T>,
): Promise<T> {
  const run = async () => {
    await refreshPortalCsrf()
    return operation()
  }
  if (typeof navigator !== 'undefined' && navigator.locks) {
    return navigator.locks.request(
      `gps-portal-csrf:${location.origin}`,
      { mode: 'exclusive' },
      run,
    )
  }
  return run()
}

export function getStoredOrganizationId() {
  return tabOrganizationStore?.get() ?? null
}

export function storeOrganizationId(id: string | null) {
  tabOrganizationStore?.set(id)
}

/** Same-origin session client. Browser credentials are HttpOnly cookies; no bearer token is persisted. */
export function createPortalApiClient(context: PortalApiContext = {}) {
  const client = createClient<paths>({
    baseUrl: new URL(
      import.meta.env.VITE_API_BASE_URL ?? '/v1',
      typeof window === 'undefined'
        ? 'http://localhost'
        : window.location.origin,
    ).toString(),
    fetch: (request) => globalThis.fetch(request),
  })

  const portalContext: Middleware = {
    async onRequest({ request }) {
      const organizationId =
        context.organizationId?.() ?? getStoredOrganizationId()
      // Generated operation params may carry the tenant captured when the query
      // started. Preserve it across a later tab switch to prevent late requests
      // from being silently rebound to a different organization.
      if (organizationId && !request.headers.has('X-Organization-Id')) {
        request.headers.set('X-Organization-Id', organizationId)
      }

      if (!['GET', 'HEAD', 'OPTIONS'].includes(request.method.toUpperCase())) {
        const csrf = context.csrfToken?.() ?? getPortalCsrfToken()
        if (csrf) request.headers.set('X-CSRF-Token', csrf)
      }
      return new Request(request, { credentials: 'include' })
    },
  }

  client.use(portalContext)
  return client
}

export const portalApi = createPortalApiClient()

export function portalErrorMessage(error: unknown) {
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === 'string') return message
  }
  if (error && typeof error === 'object' && 'error' in error) {
    const envelope = (error as { error?: { message?: unknown } }).error
    if (envelope && typeof envelope.message === 'string')
      return envelope.message
  }
  return 'Permintaan gagal. Periksa koneksi dan coba lagi.'
}
