import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { lazy, Suspense, useState } from 'react'
import { BrowserRouter, Navigate, Route, Routes } from 'react-router'
import { PortalErrorBoundary } from '@/components/portal-error-boundary'
import { PortalShell } from '@/components/portal-shell'
import { RouteLoading } from '@/components/route-loading'
import { PlaceholderPage } from '@/pages/placeholder-page'
import { PortalProvider, usePortalAuth } from '@/portal-context'
import {
  AcceptInvitationPage,
  ActivatePage,
  ForgotPasswordPage,
  LoginPage,
  MfaChallengePage,
  MfaEnrollmentPage,
  ResetPasswordPage,
} from '@/pages/auth-pages'
import { OrganizationPage } from '@/pages/organization-page'
import { SessionsPage } from '@/pages/sessions-page'
import { PlatformOrganizationsPage } from '@/features/organization/platform-organizations-page'

const OverviewPage = lazy(() => import('@/pages/overview-page'))
const TrackingManagementPage = lazy(() =>
  import('@/features/tracking/tracking-management-page').then((module) => ({
    default: module.TrackingManagementPage,
  })),
)
const LiveMapPage = lazy(() =>
  import('@/features/live/live-map-page').then((module) => ({
    default: module.LiveMapPage,
  })),
)
const HistoryPage = lazy(() =>
  import('@/features/history/history-page').then((module) => ({
    default: module.HistoryPage,
  })),
)

function AuthenticatedPortal() {
  const auth = usePortalAuth()
  if (auth.status === 'loading') return <RouteLoading />
  if (auth.status === 'anonymous') return <Navigate replace to="/login" />
  if (auth.status === 'challenge')
    return (
      <Navigate
        replace
        to={
          auth.challenge === 'mfa_enrollment_required' ? '/mfa/enroll' : '/mfa'
        }
      />
    )
  return <PortalShell />
}

function AppRoutes() {
  return (
    <PortalErrorBoundary>
      <Suspense fallback={<RouteLoading />}>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route path="/mfa" element={<MfaChallengePage />} />
          <Route path="/mfa/enroll" element={<MfaEnrollmentPage />} />
          <Route path="/forgot-password" element={<ForgotPasswordPage />} />
          <Route path="/reset-password" element={<ResetPasswordPage />} />
          <Route path="/activate" element={<ActivatePage />} />
          <Route
            path="/accept-invitation"
            element={<AcceptInvitationPage />}
          />
          <Route path="/invite/accept" element={<AcceptInvitationPage />} />
          <Route path="/" element={<AuthenticatedPortal />}>
            <Route index element={<Navigate replace to="/overview" />} />
            <Route path="overview" element={<OverviewPage />} />
            <Route path="organization" element={<OrganizationPage />} />
            <Route
              path="platform/organizations"
              element={<PlatformOrganizationsPage />}
            />
            <Route path="sessions" element={<SessionsPage />} />
            <Route path="devices" element={<TrackingManagementPage />} />
            <Route path="live" element={<LiveMapPage />} />
            <Route path="history" element={<HistoryPage />} />
          </Route>
          <Route
            path="*"
            element={
              <PlaceholderPage
                description="Halaman yang diminta tidak ditemukan."
                title="404 — Page not found"
              />
            }
          />
        </Routes>
      </Suspense>
    </PortalErrorBoundary>
  )
}

export function App() {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: { retry: false, refetchOnWindowFocus: false },
        },
      }),
  )
  return (
    <QueryClientProvider client={queryClient}>
      <PortalProvider>
        <BrowserRouter>
          <AppRoutes />
        </BrowserRouter>
      </PortalProvider>
    </QueryClientProvider>
  )
}
