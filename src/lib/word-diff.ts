/** A stretch of text that stayed, was taken out, or was put in. */
export interface Change {
  kind: 'same' | 'gone' | 'new'
  text: string
}

/** Words with the marks that stand against them, the spaces between them, and each
 * character of a writing without spaces on its own. */
function pieces(text: string): string[] {
  return text.match(/[⺀-鿿가-힯豈-﫿＀-￯]|\s+|[^\s⺀-鿿가-힯豈-﫿＀-￯]+/g) ?? []
}

/** What changed from `before` to `after`, word by word. The pieces that are not gone,
 * joined, are `after`. */
export function changes(before: string, after: string): Change[] {
  const was = pieces(before)
  const is = pieces(after)
  // The longest run of pieces both have in order, from each place onward.
  const width = is.length + 1
  const kept = new Uint32Array((was.length + 1) * width)
  for (let i = was.length - 1; i >= 0; i -= 1) {
    for (let j = is.length - 1; j >= 0; j -= 1) {
      kept[i * width + j] =
        was[i] === is[j]
          ? kept[(i + 1) * width + j + 1] + 1
          : Math.max(kept[(i + 1) * width + j], kept[i * width + j + 1])
    }
  }
  const out: Change[] = []
  let gone = ''
  let fresh = ''
  const put = (kind: Change['kind'], text: string) => {
    if (!text) return
    const last = out[out.length - 1]
    if (last?.kind === kind) last.text += text
    else out.push({ kind, text })
  }
  const settle = () => {
    put('gone', gone)
    put('new', fresh)
    gone = ''
    fresh = ''
  }
  let i = 0
  let j = 0
  while (i < was.length || j < is.length) {
    if (i < was.length && j < is.length && was[i] === is[j]) {
      settle()
      put('same', was[i])
      i += 1
      j += 1
    } else if (
      j >= is.length ||
      (i < was.length && kept[(i + 1) * width + j] >= kept[i * width + j + 1])
    ) {
      gone += was[i]
      i += 1
    } else {
      fresh += is[j]
      j += 1
    }
  }
  settle()
  return out
}
