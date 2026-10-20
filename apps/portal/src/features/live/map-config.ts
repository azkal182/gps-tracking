export type MapEnvironment = {
  VITE_MAP_STYLE_URL?: string
  VITE_MAPTILER_KEY?: string
}

export function getMapStyleUrl(env: MapEnvironment) {
  const configuredStyle = env.VITE_MAP_STYLE_URL?.trim()
  if (configuredStyle) return configuredStyle
  const key = env.VITE_MAPTILER_KEY?.trim()
  return key
    ? `https://api.maptiler.com/maps/streets-v2/style.json?key=${encodeURIComponent(key)}`
    : null
}
