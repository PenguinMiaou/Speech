import { useState, useEffect, useCallback } from 'react'

export type Route = 'home' | 'meeting' | 'settings' | 'history'

/** A window event, with the route as its detail, sent when the page in view is chosen
 * again in the sidebar: the page goes back to where it begins. */
export const NAV_AGAIN = 'nav:again'

export function parseHash(): Route {
  const hash = window.location.hash.replace('#/', '')
  if (hash === 'history' || hash === 'meeting') return hash
  if (hash === 'settings' || hash.startsWith('settings?')) return 'settings'
  return 'home'
}

export function useRoute() {
  const [route, setRoute] = useState<Route>(parseHash)

  useEffect(() => {
    const onHashChange = () => setRoute(parseHash())
    window.addEventListener('hashchange', onHashChange)
    return () => window.removeEventListener('hashchange', onHashChange)
  }, [])

  const navigate = useCallback((r: Route) => {
    window.location.hash = r === 'home' ? '#/' : `#/${r}`
  }, [])

  return { route, navigate }
}
