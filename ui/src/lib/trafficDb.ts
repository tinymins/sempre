export type TrafficDimension = 'device' | 'user' | 'host' | 'outbound' | 'process'
export interface TrafficRecord { id?: number; time: number; dimension: TrafficDimension; label: string; download: number; upload: number }

const DB_NAME = 'sempre-traffic-v1'
const STORE = 'records'

function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1)
    request.onupgradeneeded = () => {
      const store = request.result.createObjectStore(STORE, { keyPath: 'id', autoIncrement: true })
      store.createIndex('time', 'time')
      store.createIndex('dimension', 'dimension')
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}

export async function addTraffic(records: TrafficRecord[]) {
  if (!records.length) return
  const db = await database()
  await new Promise<void>((resolve, reject) => {
    const transaction = db.transaction(STORE, 'readwrite')
    for (const record of records) transaction.objectStore(STORE).add(record)
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error)
  })
  db.close()
}

export async function aggregateTraffic(since: number, dimension: TrafficDimension) {
  const db = await database()
  const records = await new Promise<TrafficRecord[]>((resolve, reject) => {
    const transaction = db.transaction(STORE, 'readonly')
    const request = transaction.objectStore(STORE).index('time').getAll(IDBKeyRange.lowerBound(since))
    request.onsuccess = () => resolve(request.result as TrafficRecord[])
    request.onerror = () => reject(request.error)
  })
  db.close()
  const totals = new Map<string, { label: string; download: number; upload: number }>()
  for (const record of records) {
    if (record.dimension !== dimension) continue
    const current = totals.get(record.label) || { label: record.label, download: 0, upload: 0 }
    current.download += record.download
    current.upload += record.upload
    totals.set(record.label, current)
  }
  return [...totals.values()].sort((left, right) => right.download + right.upload - left.download - left.upload)
}

export async function clearTraffic() {
  const db = await database()
  await new Promise<void>((resolve, reject) => {
    const transaction = db.transaction(STORE, 'readwrite')
    transaction.objectStore(STORE).clear()
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error)
  })
  db.close()
}
