export function parseUTCDate(dateStr: string) {
  let dStr = dateStr.trim();
  
  // Replace space with T to make it ISO 8601
  if (!dStr.includes('T')) {
    dStr = dStr.replace(' ', 'T');
  }
  
  // If no timezone indicator at the end (Z or +/-00:00), it implies UTC in our SQLite DB.
  // Add 'Z' so JS parses it as UTC instead of local time.
  if (!/(Z|[+-]\d{2}:?(?:\d{2})?)$/.test(dStr)) {
     dStr += 'Z';
  }
  
  return new Date(dStr);
}

export function formatIST(dateStr: string, includeTime = false) {
  const d = parseUTCDate(dateStr);
  
  if (includeTime) {
    return d.toLocaleString('en-IN', { timeZone: 'Asia/Kolkata', dateStyle: 'medium', timeStyle: 'short' });
  }
  return d.toLocaleDateString('en-IN', { timeZone: 'Asia/Kolkata', dateStyle: 'medium' });
}
