const sqlite3 = require('sqlite3').verbose();
const path = require('path');

const dbPath = path.join(process.env.APPDATA, 'com.progresslens.dev', 'progress_lens.db');

const db = new sqlite3.Database(dbPath, sqlite3.OPEN_READONLY, (err) => {
    if (err) {
        console.error("Error opening db:", err.message);
        return;
    }
    db.all("SELECT * FROM fields", [], (err, rows) => {
        if (err) {
            console.error(err);
        } else {
            console.log("Total fields:", rows.length);
            console.log("Empty key fields:", rows.filter(r => !r.sheet_key).length);
        }
        db.close();
    });
});
