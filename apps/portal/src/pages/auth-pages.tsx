import {
  Children,
  cloneElement,
  isValidElement,
  useEffect,
  useState,
  type ReactNode,
} from 'react'
import { Link, Navigate, useNavigate } from 'react-router'
import { useForm, type UseFormRegister } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import {
  portalApi,
  portalErrorMessage,
  setPortalCsrfToken,
  withPortalMutation,
} from '@/api/client'
import { usePortalAuth } from '@/portal-context'
import { Button } from '@/components/ui/button'

const loginSchema = z.object({
  email: z.email(),
  password: z.string().min(15).max(128),
})
const codeSchema = z.object({ code: z.string().min(6).max(8) })
const recoverySchema = z.object({ recovery_code: z.string().min(12).max(80) })
const passwordSchema = z.object({ new_password: z.string().min(15).max(128) })
const emailSchema = z.object({ email: z.email() })
const enrollmentSchema = z.object({ code: z.string().length(6) })
const invitationSchema = z.object({
  password: z.string().min(15).max(128).optional(),
  display_name: z.string().max(120).optional(),
})

type FormProps<T extends z.ZodType> = {
  schema: T
  onSubmit: (value: z.infer<T>) => Promise<void>
  children: ReactNode
  className?: string
}
function bindRegisteredFields(
  node: ReactNode,
  register: UseFormRegister<Record<string, unknown>>,
): ReactNode {
  return Children.map(node, (child) => {
    if (!isValidElement<{ children?: ReactNode; name?: string }>(child))
      return child
    if (child.type === 'input' && child.props.name) {
      const registration = register(child.props.name as never)
      return cloneElement(child, { ...registration, ...child.props } as never)
    }
    if (child.props.children)
      return cloneElement(
        child,
        undefined,
        bindRegisteredFields(child.props.children, register),
      )
    return child
  })
}

function Form<T extends z.ZodType>({
  schema,
  onSubmit,
  children,
  className,
}: FormProps<T>) {
  const form = useForm<Record<string, unknown>>({
    resolver: zodResolver(schema as never) as never,
  })
  const [error, setError] = useState<string | null>(null)
  return (
    <form
      noValidate
      className={className ?? 'space-y-4'}
      onSubmit={form.handleSubmit(async (value) => {
        setError(null)
        try {
          await onSubmit(value as z.infer<T>)
        } catch (e) {
          setError(portalErrorMessage(e))
        }
      })}
    >
      {bindRegisteredFields(children, form.register)}
      {error && (
        <p role="alert" className="text-destructive text-sm">
          {error}
        </p>
      )}
      {Object.keys(form.formState.errors).map((key) => (
        <p className="text-destructive text-sm" key={key}>
          {String(
            form.formState.errors[key as keyof typeof form.formState.errors]
              ?.message ?? 'Input tidak valid.',
          )}
        </p>
      ))}
      <button
        className="bg-primary text-primary-foreground hover:bg-primary/90 h-9 w-full rounded-md px-4 text-sm font-medium disabled:opacity-60"
        disabled={form.formState.isSubmitting}
        type="submit"
      >
        {form.formState.isSubmitting ? 'Memproses…' : 'Lanjutkan'}
      </button>
    </form>
  )
}

function AuthCard({
  title,
  children,
}: {
  title: string
  children: React.ReactNode
}) {
  return (
    <main className="bg-muted/30 grid min-h-screen place-items-center p-5">
      <section className="bg-background w-full max-w-md space-y-6 rounded-xl border p-6 shadow-sm">
        <header>
          <p className="text-muted-foreground text-sm">Tracker management</p>
          <h1 className="mt-1 text-2xl font-semibold">{title}</h1>
        </header>
        {children}
      </section>
    </main>
  )
}

export function LoginPage() {
  const auth = usePortalAuth()
  const navigate = useNavigate()
  if (auth.status === 'authenticated')
    return <Navigate replace to="/overview" />
  return (
    <AuthCard title="Masuk">
      <Form
        schema={loginSchema}
        onSubmit={async (v) => {
          const next = await auth.login(v.email, v.password)
          navigate(next ? '/mfa' : '/overview')
        }}
      >
        <label className="block space-y-1.5 text-sm">
          Email
          <input
            name="email"
            className="bg-background h-10 w-full rounded-md border px-3"
            type="email"
            autoComplete="username"
          />
        </label>
        <label className="block space-y-1.5 text-sm">
          Password
          <input
            name="password"
            className="bg-background h-10 w-full rounded-md border px-3"
            type="password"
            autoComplete="current-password"
          />
        </label>
      </Form>
      <div className="flex justify-between text-sm">
        <Link className="text-primary underline" to="/forgot-password">
          Lupa password?
        </Link>
        <Link className="text-primary underline" to="/mfa">
          Masukkan kode MFA
        </Link>
      </div>
    </AuthCard>
  )
}

export function MfaChallengePage() {
  const auth = usePortalAuth()
  if (auth.status !== 'challenge') return <Navigate replace to="/login" />
  const isEnrollment = auth.challenge === 'mfa_enrollment_required'
  return (
    <AuthCard title={isEnrollment ? 'Siapkan MFA' : 'Verifikasi MFA'}>
      {isEnrollment ? (
        <p className="text-muted-foreground text-sm">
          Akun ini perlu mengaktifkan MFA sebelum akses portal. Lanjutkan ke
          enrollment terproteksi.
        </p>
      ) : (
        <>
          <Form schema={codeSchema} onSubmit={(v) => auth.verifyMfa(v.code)}>
            <label className="block space-y-1.5 text-sm">
              Kode TOTP
              <input
                name="code"
                className="bg-background h-10 w-full rounded-md border px-3"
                autoComplete="one-time-code"
                inputMode="numeric"
              />
            </label>
          </Form>
          <details className="rounded-lg border p-4">
            <summary className="cursor-pointer text-sm font-medium">
              Gunakan recovery code
            </summary>
            <Form
              className="mt-4 space-y-4"
              schema={recoverySchema}
              onSubmit={(v) => auth.recoverMfa(v.recovery_code)}
            >
              <label className="block space-y-1.5 text-sm">
                Recovery code
                <input
                  name="recovery_code"
                  className="bg-background h-10 w-full rounded-md border px-3"
                  autoComplete="off"
                />
              </label>
            </Form>
          </details>
        </>
      )}
      {isEnrollment && (
        <Link
          className="bg-primary text-primary-foreground inline-flex h-9 items-center rounded-md px-4 text-sm"
          to="/mfa/enroll"
        >
          Mulai enrollment MFA
        </Link>
      )}
      <Button variant="outline" onClick={() => void auth.logout()}>
        Batalkan dan keluar
      </Button>
    </AuthCard>
  )
}

export function MfaEnrollmentPage() {
  const auth = usePortalAuth()
  const [enrollment, setEnrollment] = useState<{
    secret: string
    otpauth_uri: string
  } | null>(null)
  const [codes, setCodes] = useState<string[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  useEffect(() => {
    let active = true
    void withPortalMutation(() =>
      portalApi.POST('/auth/mfa/enrollment', { body: {} }),
    ).then(({ data, error: apiError }) => {
      if (!active) return
      if (apiError || !data) setError(portalErrorMessage(apiError))
      else setEnrollment(data)
    })
    return () => {
      active = false
    }
  }, [])
  if (auth.status === 'anonymous' || auth.status === 'loading')
    return <Navigate replace to="/login" />
  return (
    <AuthCard title={codes ? 'Simpan recovery codes' : 'Aktifkan MFA'}>
      {error && (
        <p role="alert" className="text-destructive text-sm">
          {error}
        </p>
      )}
      {codes ? (
        <>
          <p className="text-sm">
            Kode hanya ditampilkan sekali. Simpan di tempat aman sebelum
            melanjutkan.
          </p>
          <ul className="grid grid-cols-2 gap-2 font-mono text-sm">
            {codes.map((code) => (
              <li className="rounded border p-2" key={code}>
                {code}
              </li>
            ))}
          </ul>
          <Button
            onClick={() => {
              setCodes(null)
              setEnrollment(null)
              void auth.refreshMe()
              navigate('/overview')
            }}
          >
            Saya sudah menyimpannya
          </Button>
        </>
      ) : (
        <>
          {enrollment && (
            <div className="space-y-3 text-sm">
              <p>
                Tambahkan akun ini ke aplikasi authenticator melalui URI atau
                masukkan secret berikut:
              </p>
              <code className="bg-muted block rounded p-3 break-all">
                {enrollment.secret}
              </code>
              <a
                className="text-primary underline"
                href={enrollment.otpauth_uri}
              >
                Buka authenticator
              </a>
            </div>
          )}
          <Form
            schema={enrollmentSchema}
            onSubmit={async (v) => {
              const { data, error: apiError } = await withPortalMutation(() =>
                portalApi.POST('/auth/mfa/enrollment/confirm', { body: v }),
              )
              if (apiError || !data)
                throw apiError ?? new Error('Verifikasi kode gagal')
              setPortalCsrfToken(null)
              setCodes(data.recovery_codes)
            }}
          >
            <label className="block space-y-1.5 text-sm">
              Kode 6 digit
              <input
                name="code"
                className="bg-background h-10 w-full rounded-md border px-3"
                inputMode="numeric"
                autoComplete="one-time-code"
              />
            </label>
          </Form>
        </>
      )}
    </AuthCard>
  )
}

export function ForgotPasswordPage() {
  const [sent, setSent] = useState(false)
  return (
    <AuthCard title="Reset password">
      {sent ? (
        <p className="text-sm">
          Jika akun memenuhi syarat, instruksi akan dikirim melalui email.
        </p>
      ) : (
        <Form
          schema={emailSchema}
          onSubmit={async ({ email }) => {
            const { error } = await withPortalMutation(() =>
              portalApi.POST('/auth/password/forgot', { body: { email } }),
            )
            if (error) throw error
            setSent(true)
          }}
        >
          <label className="block space-y-1.5 text-sm">
            Email
            <input
              name="email"
              className="bg-background h-10 w-full rounded-md border px-3"
              type="email"
              autoComplete="email"
            />
          </label>
        </Form>
      )}
      <Link className="text-primary text-sm underline" to="/login">
        Kembali ke login
      </Link>
    </AuthCard>
  )
}

function useOneTimeToken() {
  const [token] = useState(() => {
    const url = new URL(window.location.href)
    const fragmentParams = new URLSearchParams(url.hash.slice(1))
    const value =
      url.searchParams.get('token') ?? fragmentParams.get('token') ?? ''
    if (value) {
      url.searchParams.delete('token')
      url.hash = ''
      window.history.replaceState({}, '', `${url.pathname}${url.search}`)
    }
    return value
  })
  return token
}

export function ActivatePage() {
  const token = useOneTimeToken()
  const [done, setDone] = useState(false)
  return (
    <AuthCard title="Aktivasi akun">
      {done ? (
        <>
          <p className="text-sm">
            Akun diaktifkan. Silakan masuk untuk melanjutkan.
          </p>
          <Link className="text-primary underline" to="/login">
            Masuk
          </Link>
        </>
      ) : (
        <Form
          schema={passwordSchema}
          onSubmit={async ({ new_password }) => {
            if (!token) throw new Error('Token aktivasi tidak ditemukan.')
            const { error } = await withPortalMutation(() =>
              portalApi.POST('/auth/activate', {
                body: { token, new_password },
              }),
            )
            if (error) throw error
            setDone(true)
          }}
        >
          <label className="block space-y-1.5 text-sm">
            Password baru
            <input
              name="new_password"
              className="bg-background h-10 w-full rounded-md border px-3"
              type="password"
              autoComplete="new-password"
            />
          </label>
        </Form>
      )}
    </AuthCard>
  )
}

export function ResetPasswordPage() {
  const token = useOneTimeToken()
  const [done, setDone] = useState(false)
  return (
    <AuthCard title="Buat password baru">
      {done ? (
        <>
          <p className="text-sm">
            Password diperbarui. MFA tetap mengikuti pengaturan akun. Silakan
            login kembali.
          </p>
          <Link className="text-primary underline" to="/login">
            Masuk
          </Link>
        </>
      ) : (
        <Form
          schema={passwordSchema}
          onSubmit={async ({ new_password }) => {
            if (!token) throw new Error('Token reset tidak ditemukan.')
            const { error } = await withPortalMutation(() =>
              portalApi.POST('/auth/password/reset', {
                body: { token, new_password },
              }),
            )
            if (error) throw error
            setDone(true)
          }}
        >
          <label className="block space-y-1.5 text-sm">
            Password baru
            <input
              name="new_password"
              className="bg-background h-10 w-full rounded-md border px-3"
              type="password"
              autoComplete="new-password"
            />
          </label>
        </Form>
      )}
    </AuthCard>
  )
}

export function AcceptInvitationPage() {
  const token = useOneTimeToken()
  const [done, setDone] = useState<string | null>(null)
  return (
    <AuthCard title="Terima undangan">
      {done ? (
        <>
          <p className="text-sm">{done}</p>
          <Link className="text-primary underline" to="/login">
            Ke login
          </Link>
        </>
      ) : (
        <Form
          schema={invitationSchema}
          onSubmit={async (values) => {
            if (!token) throw new Error('Token undangan tidak ditemukan.')
            const body = {
              token,
              ...(values.password ? { password: values.password } : {}),
              ...(values.display_name
                ? { display_name: values.display_name }
                : {}),
            }
            const { data, error } = await withPortalMutation(() =>
              portalApi.POST('/auth/invitations/accept', { body }),
            )
            if (error || !data)
              throw error ?? new Error('Undangan tidak dapat diterima')
            setDone(
              data.state === 'login_required'
                ? 'Undangan diterima. Login untuk mengaktifkan akses.'
                : 'Undangan diterima. Jika akses memerlukan MFA, enrollment akan diminta setelah login.',
            )
          }}
        >
          <label className="block space-y-1.5 text-sm">
            Nama tampilan (akun baru)
            <input
              name="display_name"
              className="bg-background h-10 w-full rounded-md border px-3"
              autoComplete="name"
            />
          </label>
          <label className="block space-y-1.5 text-sm">
            Password baru (akun baru)
            <input
              name="password"
              className="bg-background h-10 w-full rounded-md border px-3"
              type="password"
              autoComplete="new-password"
            />
          </label>
        </Form>
      )}
    </AuthCard>
  )
}
