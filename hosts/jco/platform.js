let audit = [];
let notes = new Map();
let bodyLimit = 0n;
let origins = [];
let preopenLabels = new Map();

export function configure(options) {
  audit = [];
  notes = new Map(Object.entries(options.notes));
  bodyLimit = BigInt(options.bodyLimit);
  origins = [...options.origins];
  preopenLabels = new Map(Object.entries(options.preopenLabels ?? {}));
}

export async function read(name) {
  return notes.get(name) ?? "";
}

export function log(event) {
  audit.push(event);
}

export function requestBodyLimit() {
  return bodyLimit;
}

export function allowedOrigins() {
  return [...origins];
}

export function preopenLabel(name) {
  return preopenLabels.get(name) ?? "";
}

export function auditRecords() {
  return [...audit];
}
