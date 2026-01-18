CREATE TABLE IF NOT EXISTS wallets (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    label                  TEXT    NOT NULL UNIQUE,
    address                TEXT    NOT NULL UNIQUE,
    network                TEXT    NOT NULL CHECK (network IN ('bitcoin', 'testnet4', 'testnet', 'signet', 'regtest')),
    encrypted_mnemonic     TEXT,
    encrypted_private_key  TEXT    NOT NULL,
    encryption_method      TEXT    NOT NULL,
    imported               INTEGER NOT NULL,
    original_import_method TEXT,
    import_method          TEXT,
    created_at             INTEGER NOT NULL
);
