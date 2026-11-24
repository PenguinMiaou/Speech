import type { Transition, Variants } from 'framer-motion'

/** The app's one motion curve: quick to start, soft to settle. The same as --ease-out in
 * globals.css. */
export const ease = [0.2, 0, 0, 1] as const

/** How long a change takes, in seconds: small things, most things, and whole panels. The
 * same as --duration-fast, --duration-base and --duration-slow in globals.css. */
export const duration = { fast: 0.14, base: 0.2, slow: 0.3 }

/** A spring that comes to rest without overshooting, for things that move or resize. */
const settle: Transition = { type: 'spring', stiffness: 420, damping: 40, mass: 1 }
/** The same, slower, for larger things. */
const glide: Transition = { type: 'spring', stiffness: 260, damping: 32, mass: 1 }

/** The two springs the app moves with, so that each part of it moves the same way. */
export const spring = {
  smooth: settle,
  gentle: glide,
}

/** A timed change in the app's curve. */
export const timed = (length: keyof typeof duration = 'base'): Transition => ({
  duration: duration[length],
  ease,
})

export const fadeIn: Variants = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: timed() },
  exit: { opacity: 0, transition: timed('fast') },
}

export const scaleIn: Variants = {
  initial: { opacity: 0, scale: 0.98 },
  animate: { opacity: 1, scale: 1, transition: timed() },
  exit: { opacity: 0, scale: 0.98, transition: timed('fast') },
}

export const slideUp: Variants = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0, transition: timed() },
  exit: { opacity: 0, y: -4, transition: timed('fast') },
}

export const slideRight: Variants = {
  initial: { opacity: 0, x: 8 },
  animate: { opacity: 1, x: 0, transition: timed() },
  exit: { opacity: 0, x: -8, transition: timed('fast') },
}

/** A small shake, to say no. */
export const jellyShake: Variants = {
  animate: {
    x: [-2, 2, -1, 1, 0],
    transition: { duration: 0.3, ease: 'easeInOut' },
  },
}
