import type { Transition, Variants } from 'framer-motion'

// Spring config — smooth, no visible bounce
export const spring = {
  snappy: { type: 'spring', stiffness: 400, damping: 35 } as Transition,
  smooth: { type: 'spring', stiffness: 300, damping: 30 } as Transition,
  gentle: { type: 'spring', stiffness: 200, damping: 26 } as Transition,
  bouncy: { type: 'spring', stiffness: 400, damping: 28 } as Transition,
  // Jelly springs — smooth with minimal overshoot
  jelly: { type: 'spring', stiffness: 300, damping: 24 } as Transition,
  jellyGentle: { type: 'spring', stiffness: 200, damping: 22 } as Transition,
  jellyBouncy: { type: 'spring', stiffness: 350, damping: 22 } as Transition,
}

// Shared animation presets
export const fadeIn: Variants = {
  initial: { opacity: 0 },
  animate: { opacity: 1 },
  exit: { opacity: 0 },
}

export const scaleIn: Variants = {
  initial: { opacity: 0, scale: 0.95 },
  animate: { opacity: 1, scale: 1 },
  exit: { opacity: 0, scale: 0.95 },
}

export const slideUp: Variants = {
  initial: { opacity: 0, y: 8 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -8 },
}

export const slideRight: Variants = {
  initial: { opacity: 0, x: 12 },
  animate: { opacity: 1, x: 0 },
  exit: { opacity: 0, x: -12 },
}

// Jelly variants — gentle, not jarring
export const jellyShake: Variants = {
  animate: {
    x: [-2, 2, -1, 1, 0],
    transition: { duration: 0.4, ease: 'easeInOut' },
  },
}
