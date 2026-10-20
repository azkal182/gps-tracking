import { Component, type ReactNode } from 'react'

type Props = { children: ReactNode }
type State = { hasError: boolean }

export class PortalErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false }

  static getDerivedStateFromError(): State {
    return { hasError: true }
  }

  render() {
    if (this.state.hasError) {
      return (
        <main className="grid min-h-screen place-items-center p-6">
          <section
            aria-labelledby="portal-error-title"
            className="max-w-lg text-center"
            role="alert"
          >
            <h1 className="text-2xl font-semibold" id="portal-error-title">
              Halaman tidak dapat ditampilkan
            </h1>
            <p className="text-muted-foreground mt-3">
              Terjadi kesalahan saat merender portal. Muat ulang halaman untuk
              mencoba lagi.
            </p>
            <a className="text-primary mt-6 inline-block underline" href="/">
              Muat ulang portal
            </a>
          </section>
        </main>
      )
    }

    return this.props.children
  }
}
