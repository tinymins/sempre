export function parseJSONC<T>(input: string): T {
  let output = ''
  let inString = false
  let escaped = false
  for (let index = 0; index < input.length; index += 1) {
    const character = input[index]
    const next = input[index + 1]
    if (inString) {
      output += character
      if (escaped) escaped = false
      else if (character === '\\') escaped = true
      else if (character === '"') inString = false
      continue
    }
    if (character === '"') { inString = true; output += character; continue }
    if (character === '/' && next === '/') {
      while (index < input.length && input[index] !== '\n') index += 1
      output += '\n'
      continue
    }
    if (character === '/' && next === '*') {
      index += 2
      while (index < input.length && !(input[index] === '*' && input[index + 1] === '/')) index += 1
      index += 1
      continue
    }
    output += character
  }
  return JSON.parse(removeTrailingCommas(output)) as T
}

function removeTrailingCommas(input: string) {
  let output = ''
  let inString = false
  let escaped = false
  for (let index = 0; index < input.length; index += 1) {
    const character = input[index]
    if (inString) {
      output += character
      if (escaped) escaped = false
      else if (character === '\\') escaped = true
      else if (character === '"') inString = false
      continue
    }
    if (character === '"') { inString = true; output += character; continue }
    if (character === ',') {
      let cursor = index + 1
      while (/\s/.test(input[cursor] || '')) cursor += 1
      if (input[cursor] === '}' || input[cursor] === ']') continue
    }
    output += character
  }
  return output
}
