import { portalErrorMessage } from '@/api/client'

export function historyErrorMessage(error: unknown) {
  const status =
    error && typeof error === 'object' && 'status' in error
      ? Number((error as { status?: unknown }).status)
      : 0
  if (
    status === 403 ||
    (error &&
      typeof error === 'object' &&
      'error' in error &&
      (error as { error?: { code?: string } }).error?.code === 'FORBIDDEN')
  )
    return 'Akses histori tidak diberikan untuk akun ini.'
  if (status === 404)
    return 'Device atau sesi tidak ditemukan pada organisasi aktif.'
  return portalErrorMessage(error)
}
