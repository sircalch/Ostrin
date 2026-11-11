// IndexedDB adapter for the Run History. It reads and writes only what workbench-history-storage.js prepares.
//
// Versions: DB_VERSION is the IndexedDB schema version of this database. It is not the record format (RECORD_SCHEMA)
// nor the run-entry format (ostrin.history/v1). Migrations are explicit: MIGRATIONS[n] moves the database from version
// n to version n + 1, and an upgrade applies every step it is missing, in order.
//
// Stores:
//   records   key: run id            value: { schema, entry, missing, storedAt }
//   contents  key: SHA-256 digest    value: { bytes: Uint8Array, size }   (content-addressed, stored once)
//
// Every run is written in one readwrite transaction: its contents and its record are committed together or not at
// all. Writes and reads are serialised, so a load never sees a half-written run and writes keep their order.

import {
  classifyStorageError,
  createSerialQueue,
  ERROR_CODES,
  prepareRecord,
  restoreRecords,
  StorageError,
} from "./workbench-history-storage.js";

export const DB_NAME = "ostrin-workbench-history";
export const DB_VERSION = 1;
export const STORES = Object.freeze({ records: "records", contents: "contents" });

const MIGRATIONS = [
  // Version 1: the two stores above.
  (db) => {
    db.createObjectStore(STORES.records);
    db.createObjectStore(STORES.contents);
  },
];

function unavailable(error) {
  const reason = classifyStorageError(error).message;
  return new StorageError(ERROR_CODES.unavailable, "saved history is unavailable: " + reason);
}

export function openDatabase(factory, name = DB_NAME, version = DB_VERSION) {
  return new Promise((resolve, reject) => {
    let request;
    try {
      request = factory.open(name, version);
    } catch (error) {
      reject(unavailable(error));
      return;
    }
    request.onupgradeneeded = (event) => {
      for (let step = event.oldVersion; step < version; step++) MIGRATIONS[step](request.result);
    };
    request.onblocked = () => reject(new StorageError(ERROR_CODES.unavailable, "saved history is unavailable: another tab holds the database open"));
    request.onerror = () => reject(unavailable(request.error));
    request.onsuccess = () => {
      const db = request.result;
      // Another tab upgrading the database must not be blocked by this one.
      db.onversionchange = () => db.close();
      resolve(db);
    };
  });
}

// Runs one transaction over both stores. `body(tx)` issues the requests and may return a function that reads their
// results; that function runs only after the transaction has committed. An error at any point aborts everything.
function transaction(db, mode, body) {
  return new Promise((resolve, reject) => {
    let tx;
    try {
      tx = db.transaction([STORES.records, STORES.contents], mode);
    } catch (error) {
      reject(classifyStorageError(error));
      return;
    }
    let collect = null;
    let failure = null;
    tx.oncomplete = () => resolve(collect ? collect() : undefined);
    tx.onabort = () => reject(classifyStorageError(tx.error ?? failure));
    try {
      collect = body(tx) ?? null;
    } catch (error) {
      failure = error;
      try {
        tx.abort();
      } catch (_) {
        // Already finished.
      }
    }
  });
}

function commitBody(prepared) {
  return (tx) => {
    const contents = tx.objectStore(STORES.contents);
    for (const item of prepared.contents) contents.put(item.value, item.key);
    // A record that already exists is refused (ConstraintError) and aborts the contents written above with it.
    tx.objectStore(STORES.records).add(prepared.record, prepared.key);
  };
}

function readAllBody(tx) {
  const records = tx.objectStore(STORES.records);
  const contents = tx.objectStore(STORES.contents);
  const requests = {
    recordKeys: records.getAllKeys(),
    recordValues: records.getAll(),
    contentKeys: contents.getAllKeys(),
    contentValues: contents.getAll(),
  };
  return () => ({
    records: requests.recordKeys.result.map((key, i) => ({ key, value: requests.recordValues.result[i] })),
    contents: requests.contentKeys.result.map((key, i) => ({ key, value: requests.contentValues.result[i] })),
  });
}

// The store used by the page. `factory` is an IDBFactory (indexedDB); `digestOf` computes SHA-256 over bytes.
export function createHistoryStore({ factory = globalThis.indexedDB, name = DB_NAME, digestOf } = {}) {
  const enqueue = createSerialQueue();
  let opening = null;

  function database() {
    if (!opening) {
      opening = factory
        ? openDatabase(factory, name)
        : Promise.reject(new StorageError(ERROR_CODES.unavailable, "saved history is unavailable: IndexedDB is not supported here"));
      // A failed open is not cached, so a later attempt can succeed (for example after the user frees storage).
      opening.catch(() => {
        opening = null;
      });
    }
    return opening;
  }

  return {
    // Restores every stored run. Corrupt runs are quarantined individually; the rest load. Rejects when the database
    // cannot be read at all.
    async load() {
      const raw = await enqueue(async () => {
        const db = await database();
        return transaction(db, "readonly", readAllBody);
      });
      return restoreRecords(raw.records, raw.contents, digestOf);
    },

    // Stores one run with its original bytes. Resolves with the verification of what was stored. Rejects with a
    // StorageError code: "invalid" or "too-large" before anything is written, or "quota", "duplicate", "denied",
    // "unavailable" or "transaction" from the database. A rejected run leaves the store as it was.
    async record(entry, contents) {
      const prepared = await prepareRecord(entry, contents, digestOf);
      await enqueue(async () => {
        const db = await database();
        await transaction(db, "readwrite", commitBody(prepared));
      });
      return { id: entry.id, verification: prepared.verification, bytes: prepared.bytes };
    },
  };
}
