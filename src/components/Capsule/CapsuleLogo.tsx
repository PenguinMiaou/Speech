/** The five bars of the app icon, in the current text colour. */
export function CapsuleLogo({ size = 22, className = '' }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      className={className}
      aria-hidden="true"
    >
      <rect x="3.6" y="9.8" width="2.2" height="4.4" rx="1.1" />
      <rect x="7.2" y="6.6" width="2.2" height="10.8" rx="1.1" />
      <rect x="10.9" y="4" width="2.2" height="16" rx="1.1" />
      <rect x="14.6" y="7.6" width="2.2" height="8.8" rx="1.1" />
      <rect x="18.2" y="10" width="2.2" height="4" rx="1.1" />
    </svg>
  )
}
