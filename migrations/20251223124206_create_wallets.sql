CREATE TABLE IF NOT EXISTS wallets (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    label                  TEXT    NOT NULL UNIQUE,
    address                TEXT    NOT NULL UNIQUE,
    network                TEXT    NOT NULL,
    encrypted_mnemonic     TEXT,
    encrypted_private_key  TEXT,
    created_at             TEXT    NOT NULL,
    encryption_method      TEXT    NOT NULL,
    imported               INTEGER,
    import_method          TEXT
);
