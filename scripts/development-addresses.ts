const services = [
  ['Sempre API', 'http://127.0.0.1:33212'],
  ['Control UI', 'http://127.0.0.1:5173'],
  ['Website', 'http://127.0.0.1:4174'],
]

console.log('\nSempre development servers\n')
for (const [name, address] of services) console.log(`  ${name.padEnd(12)} ${address}`)
console.log('')
