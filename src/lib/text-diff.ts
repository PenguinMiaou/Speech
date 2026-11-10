export type DiffPart = { kind: 'same' | 'removed' | 'added'; text: string }

const MAX_COMPARED_CHARACTERS = 1000

function pushPart(parts: DiffPart[], kind: DiffPart['kind'], text: string) {
  const last = parts[parts.length - 1]
  if (last?.kind === kind) {
    last.text += text
  } else {
    parts.push({ kind, text })
  }
}

/** Compares two texts character by character, through their longest common subsequence. */
export function diffText(before: string, after: string): DiffPart[] {
  const from = Array.from(before)
  const to = Array.from(after)
  const parts: DiffPart[] = []

  if (from.length > MAX_COMPARED_CHARACTERS || to.length > MAX_COMPARED_CHARACTERS) {
    if (before) pushPart(parts, 'removed', before)
    if (after) pushPart(parts, 'added', after)
    return parts
  }

  const width = to.length + 1
  const common = new Uint16Array((from.length + 1) * width)
  for (let i = from.length - 1; i >= 0; i--) {
    for (let j = to.length - 1; j >= 0; j--) {
      common[i * width + j] =
        from[i] === to[j]
          ? common[(i + 1) * width + j + 1] + 1
          : Math.max(common[(i + 1) * width + j], common[i * width + j + 1])
    }
  }

  let i = 0
  let j = 0
  while (i < from.length && j < to.length) {
    if (from[i] === to[j]) {
      pushPart(parts, 'same', from[i])
      i++
      j++
    } else if (common[(i + 1) * width + j] >= common[i * width + j + 1]) {
      pushPart(parts, 'removed', from[i++])
    } else {
      pushPart(parts, 'added', to[j++])
    }
  }
  while (i < from.length) pushPart(parts, 'removed', from[i++])
  while (j < to.length) pushPart(parts, 'added', to[j++])
  return parts
}
