const sheetId = '1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms';
fetch(`https://sheets.googleapis.com/v4/spreadsheets/${sheetId}?includeGridData=true&ranges=A:ZZ&alt=json&prettyPrint=false`, {
  headers: { 'Cache-Control': 'no-cache', 'Pragma': 'no-cache' }
}).then(res => res.text()).then(txt => console.log(txt.slice(0, 1000) + '...'));
