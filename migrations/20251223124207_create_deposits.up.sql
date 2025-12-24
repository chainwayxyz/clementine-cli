CREATE TABLE IF NOT EXISTS deposits (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    deposit_address          TEXT    NOT NULL UNIQUE,
    aggregated_public_key    TEXT    NOT NULL,
    recovery_taproot_address TEXT    NOT NULL,
    citrea_address           TEXT    NOT NULL,
    user_takes_after         INTEGER NOT NULL,
    network                  TEXT    NOT NULL,
    created_at               INTEGER NOT NULL
);
