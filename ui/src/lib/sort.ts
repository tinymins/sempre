const collator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: 'base',
})

export function compareText(left: unknown, right: unknown) {
  return collator.compare(String(left ?? ''), String(right ?? ''))
}

export function compareNumber(left: unknown, right: unknown) {
  const leftNumber = Number(left)
  const rightNumber = Number(right)
  return (Number.isFinite(leftNumber) ? leftNumber : 0) - (Number.isFinite(rightNumber) ? rightNumber : 0)
}

export function compareDate(left: unknown, right: unknown) {
  const leftTime = new Date(String(left ?? '')).valueOf()
  const rightTime = new Date(String(right ?? '')).valueOf()
  return (Number.isNaN(leftTime) ? 0 : leftTime) - (Number.isNaN(rightTime) ? 0 : rightTime)
}
