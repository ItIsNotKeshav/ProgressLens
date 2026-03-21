export function parseUTCDate(dateStr: string) {
  let dStr = dateStr;
  
  if (!dStr.includes('T')) dStr = dStr.replace(' ', 'T');
  if (!dStr.endsWith('Z') && !dStr.includes('+') && !dStr.includes('-')) {
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
