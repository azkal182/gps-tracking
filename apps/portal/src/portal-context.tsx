import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { useQuery } from '@tanstack/react-query'
import { useQueryClient } from '@tanstack/react-query'
import type { components } from './api/generated'
import {
  portalApi,
  setPortalCsrfToken,
  getStoredOrganizationId,
  storeOrganizationId,
  withPortalMutation,
} from './api/client'

type Me = components['schemas']['AuthMeResponse']
type LoginResult = components['schemas']['LoginResult']
type ChallengeState = 'mfa_required' | 'mfa_enrollment_required'
type AuthStatus = 'loading' | 'anonymous' | 'authenticated' | 'challenge'

type PortalAuthValue = {
  status: AuthStatus
  me: Me | null
  challenge: ChallengeState | null
  error: string | null
  login: (email: string, password: string) => Promise<ChallengeState | null>
  verifyMfa: (code: string) => Promise<void>
  recoverMfa: (recoveryCode: string) => Promise<void>
  refreshMe: () => Promise<void>
  logout: () => Promise<void>
  clearError: () => void
}

type TenantValue = {
  organizationId: string | null
  memberships: Me['memberships']
  selectOrganization: (id: string) => Promise<void>
}

const AuthContext = createContext<PortalAuthValue | null>(null)
const TenantContext = createContext<TenantValue | null>(null)

/* eslint-disable react-refresh/only-export-components */

async function fetchMe() {
  const { data, error } = await portalApi.GET('/auth/me')
  if (error || !data) throw error ?? new Error('Sesi tidak tersedia')
  return data
}

function isUnauthorized(error: unknown) {
  return (
    !!error &&
    typeof error === 'object' &&
    'error' in error &&
    !!(error as { error?: { code?: string } }).error?.code?.includes(
      'UNAUTHORIZED',
    )
  )
}

export function PortalProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const [status, setStatus] = useState<AuthStatus>('loading')
  const [me, setMe] = useState<Me | null>(null)
  const [challenge, setChallenge] = useState<ChallengeState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [organizationId, setOrganizationId] = useState<string | null>(
    getStoredOrganizationId,
  )
  const initRun = useRef(0)

  const clearSensitiveState = useCallback(() => {
    setPortalCsrfToken(null)
    setMe(null)
    setChallenge(null)
    setOrganizationId(null)
    storeOrganizationId(null)
    queryClient.clear()
  }, [queryClient])

  const loadMe = useCallback(async () => {
    const user = await fetchMe()
    setMe(user)
    setStatus('authenticated')
    setChallenge(null)
    const valid = user.memberships.some(
      (m) => m.tenant_id === getStoredOrganizationId() && m.status === 'active',
    )
    if (!valid) {
      const first =
        user.memberships.find((m) => m.status === 'active')?.tenant_id ?? null
      setOrganizationId(first)
      storeOrganizationId(first)
    } else {
      setOrganizationId(getStoredOrganizationId())
    }
  }, [])

  useEffect(() => {
    const run = ++initRun.current
    void fetchMe()
      .then((user) => {
        if (run !== initRun.current) return
        setMe(user)
        setStatus('authenticated')
        const stored = getStoredOrganizationId()
        const valid = user.memberships.some(
          (m) => m.tenant_id === stored && m.status === 'active',
        )
        const selected = valid
          ? stored
          : (user.memberships.find((m) => m.status === 'active')?.tenant_id ??
            null)
        setOrganizationId(selected)
        storeOrganizationId(selected)
      })
      .catch((err) => {
        if (run !== initRun.current) return
        if (isUnauthorized(err)) {
          clearSensitiveState()
          setStatus('anonymous')
        } else {
          setError('Portal tidak dapat terhubung ke server. Coba muat ulang.')
          setStatus('anonymous')
        }
      })
    return () => {
      initRun.current += 1
    }
  }, [clearSensitiveState])

  useEffect(() => {
    if (typeof BroadcastChannel === 'undefined') return
    const channel = new BroadcastChannel('gps-portal-auth')
    channel.onmessage = (event) => {
      if (event.data?.type === 'logout') {
        clearSensitiveState()
        setStatus('anonymous')
      } else if (event.data?.type === 'session-changed') {
        void loadMe().catch(() => {
          clearSensitiveState()
          setStatus('anonymous')
        })
      }
    }
    return () => channel.close()
  }, [clearSensitiveState, loadMe])

  const login = useCallback(
    async (email: string, password: string) => {
      setError(null)
      const result = await withPortalMutation(async () => {
        const { data, error: apiError } = await portalApi.POST('/auth/login', {
          body: { email, password },
        })
        if (apiError || !data) throw apiError ?? new Error('Login gagal')
        return data as LoginResult
      })
      if (result.state === 'authenticated') await loadMe()
      else {
        setChallenge(result.state)
        setStatus('challenge')
      }
      return result.state === 'authenticated' ? null : result.state
      if (
        result.state === 'authenticated' &&
        typeof BroadcastChannel !== 'undefined'
      ) {
        const channel = new BroadcastChannel('gps-portal-auth')
        channel.postMessage({ type: 'session-changed' })
        channel.close()
      }
    },
    [loadMe],
  )

  const verifyMfa = useCallback(
    async (code: string) => {
      setError(null)
      await withPortalMutation(async () => {
        const { error: apiError } = await portalApi.POST('/auth/mfa/verify', {
          body: { code },
        })
        if (apiError) throw apiError
      })
      await loadMe()
      if (typeof BroadcastChannel !== 'undefined') {
        const channel = new BroadcastChannel('gps-portal-auth')
        channel.postMessage({ type: 'session-changed' })
        channel.close()
      }
    },
    [loadMe],
  )

  const recoverMfa = useCallback(async (recoveryCode: string) => {
    setError(null)
    const data = await withPortalMutation(async () => {
      const { data, error: apiError } = await portalApi.POST(
        '/auth/mfa/recover',
        { body: { recovery_code: recoveryCode } },
      )
      if (apiError || !data) throw apiError ?? new Error('Pemulihan MFA gagal')
      return data
    })
    setChallenge(data.state)
    setStatus('challenge')
  }, [])

  const refreshMe = useCallback(async () => {
    try {
      await loadMe()
    } catch {
      clearSensitiveState()
      setStatus('anonymous')
    }
  }, [clearSensitiveState, loadMe])

  const logout = useCallback(async () => {
    try {
      await withPortalMutation(async () => {
        await portalApi.POST('/auth/logout', { body: {} })
      })
    } finally {
      clearSensitiveState()
      setStatus('anonymous')
      if (typeof BroadcastChannel !== 'undefined') {
        const channel = new BroadcastChannel('gps-portal-auth')
        channel.postMessage({ type: 'logout' })
        channel.close()
      }
    }
  }, [clearSensitiveState])

  const selectOrganization = useCallback(
    async (id: string) => {
      if (
        !me?.memberships.some(
          (m) => m.tenant_id === id && m.status === 'active',
        )
      ) {
        throw new Error(
          'Keanggotaan organisasi tidak aktif atau tidak tersedia.',
        )
      }
      const previous = organizationId
      if (previous)
        await queryClient.cancelQueries({
          queryKey: ['organization', previous],
        })
      if (previous)
        queryClient.removeQueries({ queryKey: ['organization', previous] })
      storeOrganizationId(id)
      setOrganizationId(id)
    },
    [me, organizationId, queryClient],
  )

  const authValue = useMemo<PortalAuthValue>(
    () => ({
      status,
      me,
      challenge,
      error,
      login,
      verifyMfa,
      recoverMfa,
      refreshMe,
      logout,
      clearError: () => setError(null),
    }),
    [
      status,
      me,
      challenge,
      error,
      login,
      verifyMfa,
      recoverMfa,
      refreshMe,
      logout,
    ],
  )
  const tenantValue = useMemo<TenantValue>(
    () => ({
      organizationId,
      memberships: me?.memberships ?? [],
      selectOrganization,
    }),
    [organizationId, me, selectOrganization],
  )

  return (
    <AuthContext.Provider value={authValue}>
      <TenantContext.Provider value={tenantValue}>
        {children}
      </TenantContext.Provider>
    </AuthContext.Provider>
  )
}

export function usePortalAuth() {
  const value = useContext(AuthContext)
  if (!value)
    throw new Error('usePortalAuth must be used inside PortalProvider')
  return value
}

export function usePortalTenant() {
  const value = useContext(TenantContext)
  if (!value)
    throw new Error('usePortalTenant must be used inside PortalProvider')
  return value
}

export function organizationQueryKey<T extends readonly unknown[]>(
  organizationId: string,
  ...parts: T
) {
  return ['organization', organizationId, ...parts] as const
}

export function useOrganizationQuery<T>(
  organizationId: string | null,
  resource: string,
  queryFn: (organizationId: string, signal: AbortSignal) => Promise<T>,
  enabled = true,
) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId ?? 'none', resource),
    enabled: !!organizationId && enabled,
    queryFn: ({ signal }) => queryFn(organizationId!, signal),
  })
}
